//! [`Transport`] implementation.
//!
//! A single Transport carries **both modes**, dispatched on the multiaddr's protocol
//! segments:
//!
//! | Address | Path |
//! |---|---|
//! | `…/p2p-circuit/webrtc/p2p/<target>` | Hole punching: hand off to the paired behaviour to open a signaling stream over the relay |
//! | `/ip4/…/udp/…/webrtc-direct/certhash/…` | Direct: bind the port / dial directly, in-transport |
//!
//! The hole-punching path **does not establish the connection itself** — the signaling it
//! requires must run over an already established relay connection, and opening streams is
//! a behaviour's capability. So a dial on that path amounts to "hand the request to the
//! behaviour and wait for the result"; see [`crate::swarm::channel`]. The direct path is
//! the reverse: entirely self-contained within the transport (no signaling).
//!
//! # Dispatch is the one architectural risk
//!
//! Misclassifying either address fails silently: a direct address taken by the
//! hole-punching path waits forever for a signaling stream that never arrives, while the
//! converse dials a UDP port that does not exist. `rejects_foreign_addrs` and
//! `dispatches_by_address_family` assert both directions — read them before changing the
//! dispatch logic.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use libp2p_core::transport::{DialOpts, ListenerId, TransportError, TransportEvent};
use libp2p_core::{Multiaddr, Transport as CoreTransport};
use libp2p_identity::PeerId;

use crate::config::Config;
use crate::error::Error;
use crate::protocol::addr;
use crate::swarm::channel::{ToBehaviour, ToTransport, TransportSide};
use crate::swarm::connection::Connection;

/// The direct-mode implementation is chosen per target: native can both listen and dial,
/// the browser can only dial.
///
/// This is the only place in the crate where an implementation is selected by target — the
/// two sides share a shape, so no other code needs a `cfg`.
#[cfg(not(target_family = "wasm"))]
use crate::backend::native::direct::transport::DirectTransport;
#[cfg(target_family = "wasm")]
use crate::backend::wasm::direct::DirectTransport;

/// The connection produced by this transport.
pub type Output = (PeerId, Connection);

/// WebRTC transport (hole punching + direct).
///
/// Must be produced together with [`crate::Behaviour`] by [`crate::new`] and registered
/// with **the same** Swarm. Direct mode needs no behaviour, but both modes share one
/// Transport, so registration is unchanged.
#[derive(Debug)]
pub struct Transport {
    channel: TransportSide,
    listeners: Vec<Listener>,
    /// Transport plane for direct mode.
    direct: DirectPlane,
    pending: VecDeque<TransportEvent<BoxFuture<'static, Result<Output, Error>>, Error>>,
    /// The waker left behind when `poll` last returned Pending.
    ///
    /// `listen_on` / `remove_listener` are **external synchronous calls**; when they push
    /// events into `pending`, nothing would otherwise wake the poll — only a message on
    /// `from_behaviour` does. Without this wakeup, a new listen address would not be
    /// advertised until the next poll happened to occur for some other reason.
    waker: Option<std::task::Waker>,
}

#[derive(Debug)]
struct Listener {
    id: ListenerId,
    addr: Multiaddr,
}

/// The three states of the direct plane.
///
/// **`Disabled` and `Failed` must stay distinct.** Both make `/webrtc-direct` undialable,
/// but for opposite reasons: the former is "this mode is off, the address is left to
/// another implementation", the latter is "it is on but failed to start (broken PEM
/// certificate, not inside an async runtime)". Collapsing them into a single `None` would
/// disguise a configuration error as `MultiaddrNotSupported` — and direct is the browser's
/// only way to reach a native peer, so silently degrading costs far more than an extra
/// enum variant.
#[derive(Debug)]
enum DirectPlane {
    /// No [`crate::DirectConfig`] was supplied.
    Disabled,
    Ready(Box<DirectTransport>),
    /// Configured but failed to initialize. That must not take the whole Transport down —
    /// the hole-punching half remains usable.
    Failed(Error),
}

impl Transport {
    pub(crate) fn new(config: Config, channel: TransportSide) -> Self {
        let direct = match config.direct() {
            None => DirectPlane::Disabled,
            Some(cfg) => match DirectTransport::new(cfg) {
                Ok(t) => DirectPlane::Ready(Box::new(t)),
                Err(e) => {
                    tracing::error!("webrtc-direct 初始化失败，该模式不可用：{e}");
                    DirectPlane::Failed(e)
                }
            },
        };

        Self {
            channel,
            listeners: Vec::new(),
            direct,
            pending: VecDeque::new(),
            waker: None,
        }
    }

    /// Enqueues an event and wakes the poll.
    fn queue(&mut self, event: TransportEvent<BoxFuture<'static, Result<Output, Error>>, Error>) {
        self.pending.push_back(event);
        self.wake();
    }

    /// Wakes a pending poll.
    ///
    /// `listen_on` / `remove_listener` are external synchronous calls; without this, new
    /// events would not be delivered until the next poll happened for some other reason.
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    /// Retrieves the direct plane, distinguishing "not enabled" from "failed to start".
    fn direct_plane(
        &mut self,
        addr: &Multiaddr,
    ) -> Result<&mut DirectTransport, TransportError<Error>> {
        match &mut self.direct {
            DirectPlane::Ready(t) => Ok(t),
            // 没开就让地址原样落给注册在后面的 transport。
            DirectPlane::Disabled => Err(TransportError::MultiaddrNotSupported(addr.clone())),
            // 开了但起不来：报出真正的原因，别伪装成「不认识这个地址」。
            DirectPlane::Failed(e) => Err(TransportError::Other(Error::Connection(format!(
                "webrtc-direct 不可用：{e}"
            )))),
        }
    }

    /// Finds a listener that can accept a given inbound connection.
    ///
    /// Inbound connections arrive over the relay and are unrelated to any specific listen
    /// address, so the first surviving listener will do.
    fn listener_for_incoming(&self) -> Option<&Listener> {
        self.listeners.first()
    }
}

impl CoreTransport for Transport {
    type Output = Output;
    type Error = Error;
    type ListenerUpgrade = BoxFuture<'static, Result<Output, Error>>;
    type Dial = BoxFuture<'static, Result<Output, Error>>;

    /// A listen address has the shape `<relay-addr>/p2p-circuit/webrtc/p2p/<self>`.
    ///
    /// **The local `/p2p` segment is the caller's job** — the swarm will not add it (the
    /// relay client transport appends its own too; see its `priv_client/handler.rs`).
    /// Omitting it still yields an accepted address, but once advertised the remote cannot
    /// dial it, because the target peer cannot be parsed out.
    fn listen_on(
        &mut self,
        id: ListenerId,
        addr: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        // direct 先判：它的地址形态与打洞完全不同，没有歧义。
        if addr::is_webrtc_direct(&addr) {
            let socket = addr::parse_direct_listen(&addr)
                .ok_or_else(|| TransportError::MultiaddrNotSupported(addr.clone()))?;
            let direct = self.direct_plane(&addr)?;
            direct
                .listen_on(id, socket)
                .map_err(TransportError::Other)?;
            // 端口已绑好，通告地址在下一次 poll 时经 `DirectEvent::NewAddress` 交付。
            self.wake();
            return Ok(());
        }

        if !addr::is_webrtc(&addr) {
            return Err(TransportError::MultiaddrNotSupported(addr));
        }
        self.listeners.push(Listener {
            id,
            addr: addr.clone(),
        });
        // 留着这条：地址到底被哪个 transport 接走，只有它能证明（relay client 会
        // 静默吞下带 `/webrtc` 的 circuit 地址，没有日志就完全看不出来）。
        tracing::debug!(%addr, ?id, "accepted webrtc listen address");
        self.queue(TransportEvent::NewAddress {
            listener_id: id,
            listen_addr: addr,
        });
        Ok(())
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        if let DirectPlane::Ready(direct) = &mut self.direct
            && direct.remove_listener(id)
        {
            self.queue(TransportEvent::ListenerClosed {
                listener_id: id,
                reason: Ok(()),
            });
            return true;
        }

        let Some(idx) = self.listeners.iter().position(|l| l.id == id) else {
            return false;
        };
        self.listeners.remove(idx);
        self.queue(TransportEvent::ListenerClosed {
            listener_id: id,
            reason: Ok(()),
        });
        true
    }

    /// Dials `<relay-addr>/p2p-circuit/webrtc/p2p/<target>`.
    ///
    /// This method only delivers the request to the behaviour; the actual signaling and
    /// hole punching happen there.
    fn dial(
        &mut self,
        addr: Multiaddr,
        _opts: DialOpts,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
        if addr::is_webrtc_direct(&addr) {
            let (socket, fingerprint) = addr::parse_direct_dial(&addr)
                .ok_or_else(|| TransportError::MultiaddrNotSupported(addr.clone()))?;
            // 通配 IP 或 0 端口拨不出去——那是监听地址的形态，不是可拨地址。
            if socket.port() == 0 || socket.ip().is_unspecified() {
                return Err(TransportError::MultiaddrNotSupported(addr));
            }
            let direct = self.direct_plane(&addr)?;
            return Ok(direct.dial(socket, fingerprint));
        }

        if !addr::is_webrtc(&addr) {
            return Err(TransportError::MultiaddrNotSupported(addr));
        }
        let (signaling_addr, target) =
            addr::split(&addr).map_err(|e| TransportError::Other(Error::Addr(e)))?;

        let (tx, rx) = oneshot::channel();
        self.channel
            .to_behaviour
            .try_send(ToBehaviour::Dial {
                target,
                signaling_addr,
                result: tx,
            })
            .map_err(|_| TransportError::Other(Error::BehaviourDetached))?;

        Ok(async move {
            // behaviour 侧若在回送前被 drop，oneshot 会直接 Canceled——同样归因于装配。
            let conn = rx.await.map_err(|_| Error::BehaviourDetached)??;
            Ok((target, conn))
        }
        .boxed())
    }

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
        let this = self.get_mut();

        if let Some(event) = this.pending.pop_front() {
            return Poll::Ready(event);
        }
        this.waker = Some(cx.waker().clone());

        // direct 平面：驱动监听端口的读循环。它产出的就是 `TransportEvent`，直接透传。
        if let DirectPlane::Ready(direct) = &mut this.direct
            && let Poll::Ready(event) = direct.poll(cx)
        {
            return Poll::Ready(event);
        }

        // behaviour 完成一次入站信令 → 交给 swarm 当作 Incoming。
        // 没有这条，本端就只能主动拨、不能被拨（spec 步骤 4 的 MUST）。
        while let Poll::Ready(Some(msg)) = this.channel.from_behaviour.poll_next_unpin(cx) {
            let ToTransport::Incoming { peer, connection } = msg;
            let Some(listener) = this.listener_for_incoming() else {
                // 尚未 listen 就收到入站信令：丢弃并留痕，不是错误——
                // 对端可能在我们撤销监听的同时发起。
                tracing::debug!(%peer, "收到入站 WebRTC 连接但本端无监听器，丢弃");
                continue;
            };
            let listener_id = listener.id;
            let local_addr = listener.addr.clone();
            let send_back_addr = addr::with_peer(&local_addr, peer);
            return Poll::Ready(TransportEvent::Incoming {
                listener_id,
                upgrade: async move { Ok((peer, connection)) }.boxed(),
                local_addr,
                send_back_addr,
            });
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm::channel;
    use futures::executor::block_on;
    use futures::future::poll_fn;
    use libp2p_core::transport::PortUse;

    fn dial_opts() -> DialOpts {
        DialOpts {
            role: libp2p_core::Endpoint::Dialer,
            port_use: PortUse::Reuse,
        }
    }

    fn transport() -> (Transport, channel::BehaviourSide) {
        let (t, b) = channel::pair();
        (Transport::new(Config::default(), t), b)
    }

    fn peer(seed: u8) -> PeerId {
        libp2p_identity::Keypair::ed25519_from_bytes([seed; 32])
            .unwrap()
            .public()
            .to_peer_id()
    }

    #[test]
    fn listen_on_webrtc_addr_emits_new_address() {
        let (mut t, _b) = transport();
        let addr: Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{}/p2p-circuit/webrtc", peer(1))
            .parse()
            .unwrap();

        t.listen_on(ListenerId::next(), addr.clone()).unwrap();
        let ev = t.pending.pop_front().expect("应产生 NewAddress");
        assert!(
            matches!(ev, TransportEvent::NewAddress { listen_addr, .. } if listen_addr == addr)
        );
    }

    /// 不属于本传输的地址一律拒绝。
    ///
    /// **`/webrtc-direct` 在未配置 [`crate::DirectConfig`] 时也在此列**——那时它该留给
    /// 官方 `libp2p-webrtc` 或别的实现，我们抢下来只会让它建不成连接。
    #[test]
    fn rejects_foreign_addrs() {
        let (mut t, _b) = transport();
        for s in [
            "/ip4/1.2.3.4/tcp/4001",
            "/ip4/1.2.3.4/udp/4001/quic-v1",
            // 未启用 direct，故这两条也不该被收下
            "/ip4/1.2.3.4/udp/4001/webrtc-direct",
            "/ip4/1.2.3.4/udp/4001/webrtc-direct/certhash/uEiDikp5KVUgkLta1EjUN-IKbHk-dUBg8VzKgf5nXxLK46w",
        ] {
            let addr: Multiaddr = s.parse().unwrap();
            assert!(
                matches!(
                    t.listen_on(ListenerId::next(), addr.clone()),
                    Err(TransportError::MultiaddrNotSupported(_))
                ),
                "listen_on 不该收下 {s}"
            );
            assert!(
                matches!(
                    t.dial(addr, dial_opts()),
                    Err(TransportError::MultiaddrNotSupported(_))
                ),
                "dial 不该收下 {s}"
            );
        }
    }

    fn direct_transport() -> (Transport, channel::BehaviourSide) {
        let (t, b) = channel::pair();
        let keypair = libp2p_identity::Keypair::ed25519_from_bytes([7u8; 32]).unwrap();
        let config = Config::default().with_direct(crate::DirectConfig::new(keypair));
        (Transport::new(config, t), b)
    }

    /// 启用 direct 后，两种地址各走各的路径且**互不侵占**。
    ///
    /// 这是本 crate 唯一的分派点，认错的后果都是静默的：direct 地址被打洞路径收下
    /// 会一直等一个永远不来的信令流；打洞地址被 direct 收下会去拨一个不存在的 UDP 端口。
    #[tokio::test]
    async fn dispatches_by_address_family() {
        let (mut t, mut b) = direct_transport();
        let (relay, target) = (peer(1), peer(2));

        // 打洞地址：必须投递给 behaviour，direct 平面一无所知。
        let hole_punch: Multiaddr =
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay}/p2p-circuit/webrtc/p2p/{target}")
                .parse()
                .unwrap();
        let _dial = t.dial(hole_punch, dial_opts()).unwrap();
        assert!(
            b.from_transport.next().now_or_never().is_some(),
            "打洞地址必须走 behaviour，即便 direct 已启用"
        );

        // direct 地址：在 transport 内闭环，不碰 behaviour。
        let direct: Multiaddr = "/ip4/127.0.0.1/udp/40001/webrtc-direct/certhash/uEiDikp5KVUgkLta1EjUN-IKbHk-dUBg8VzKgf5nXxLK46w"
            .parse()
            .unwrap();
        let _dial = t.dial(direct, dial_opts()).unwrap();
        assert!(
            b.from_transport.next().now_or_never().is_none(),
            "direct 地址不该惊动 behaviour"
        );
    }

    /// 监听形态的地址不可拨——通配 IP 与 0 端口都拨不出去。
    #[tokio::test]
    async fn rejects_undialable_direct_addrs() {
        let (mut t, _b) = direct_transport();
        for s in [
            // 没有 certhash，无从校验对端证书
            "/ip4/1.2.3.4/udp/4001/webrtc-direct",
            // 通配 IP
            "/ip4/0.0.0.0/udp/4001/webrtc-direct/certhash/uEiDikp5KVUgkLta1EjUN-IKbHk-dUBg8VzKgf5nXxLK46w",
            // 0 端口
            "/ip4/1.2.3.4/udp/0/webrtc-direct/certhash/uEiDikp5KVUgkLta1EjUN-IKbHk-dUBg8VzKgf5nXxLK46w",
        ] {
            let addr: Multiaddr = s.parse().unwrap();
            assert!(
                matches!(
                    t.dial(addr, dial_opts()),
                    Err(TransportError::MultiaddrNotSupported(_))
                ),
                "{s} 不该被当成可拨地址"
            );
        }
    }

    /// 监听一个 direct 地址应当绑上真实端口，并通告带 certhash 的可拨地址。
    #[tokio::test]
    async fn direct_listen_announces_dialable_addr() {
        use libp2p_core::multiaddr::Protocol;

        let (mut t, _b) = direct_transport();
        let id = ListenerId::next();
        t.listen_on(id, "/ip4/127.0.0.1/udp/0/webrtc-direct".parse().unwrap())
            .expect("启用 direct 后应能监听");

        let ev = poll_fn(|cx| Pin::new(&mut t).poll(cx)).await;
        let TransportEvent::NewAddress { listen_addr, .. } = ev else {
            panic!("应产出 NewAddress");
        };

        // 0 端口必须被替换成系统实际分配的端口，否则通告出去对端拨不通。
        assert!(
            matches!(listen_addr.iter().nth(1), Some(Protocol::Udp(p)) if p != 0),
            "端口应是系统分配的实际值：{listen_addr}"
        );
        // certhash 必须在——没有它对端无法校验 DTLS 指纹。
        assert!(
            listen_addr
                .iter()
                .any(|p| matches!(p, Protocol::Certhash(_))),
            "通告地址必须带 certhash：{listen_addr}"
        );
        assert!(addr::is_webrtc_direct(&listen_addr));

        assert!(t.remove_listener(id));
    }

    #[test]
    fn dial_forwards_request_to_behaviour() {
        let (mut t, mut b) = transport();
        let (relay, target) = (peer(1), peer(2));
        let addr: Multiaddr =
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay}/p2p-circuit/webrtc/p2p/{target}")
                .parse()
                .unwrap();

        let _dial = t.dial(addr, dial_opts()).unwrap();

        let ToBehaviour::Dial {
            target: got,
            signaling_addr,
            ..
        } = block_on(b.from_transport.next()).expect("应收到 Dial");
        assert_eq!(got, target);
        assert!(
            !addr::is_webrtc(&signaling_addr),
            "交给 behaviour 的应是剔除 /webrtc 后的可拨 circuit 地址"
        );
    }

    /// behaviour 被 drop 后 dial 必须快速失败并明确指向装配错误，而不是永久挂起。
    #[test]
    fn dial_fails_when_behaviour_dropped() {
        let (mut t, b) = transport();
        drop(b);
        let (relay, target) = (peer(1), peer(2));
        let addr: Multiaddr =
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay}/p2p-circuit/webrtc/p2p/{target}")
                .parse()
                .unwrap();

        assert!(matches!(
            t.dial(addr, dial_opts()),
            Err(TransportError::Other(Error::BehaviourDetached))
        ));
    }

    #[test]
    fn remove_listener_reports_closed_once() {
        let (mut t, _b) = transport();
        let id = ListenerId::next();
        let addr: Multiaddr = format!("/ip4/1.2.3.4/tcp/4001/p2p/{}/p2p-circuit/webrtc", peer(1))
            .parse()
            .unwrap();

        t.listen_on(id, addr).unwrap();
        t.pending.clear();

        assert!(t.remove_listener(id));
        assert!(matches!(
            t.pending.pop_front(),
            Some(TransportEvent::ListenerClosed { .. })
        ));
        assert!(!t.remove_listener(id), "重复移除应返回 false");
    }
}
