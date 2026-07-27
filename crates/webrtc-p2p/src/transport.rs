//! [`Transport`] 实现。
//!
//! 它**不自己建连接**——建连所需的信令必须跑在一条已建立的 relay 连接上，而开流是
//! behaviour 的能力。故 dial 的实质是「把请求交给配对的 behaviour，等它回送结果」，
//! 见 [`crate::channel`]。

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use libp2p_core::transport::{DialOpts, ListenerId, TransportError, TransportEvent};
use libp2p_core::{Multiaddr, Transport as CoreTransport};
use libp2p_identity::PeerId;

use crate::channel::{ToBehaviour, ToTransport, TransportSide};
use crate::error::Error;
use crate::{Config, Connection, addr};

/// 本传输产出的连接。
pub type Output = (PeerId, Connection);

/// WebRTC 打洞传输。
///
/// 必须与 [`crate::Behaviour`] 由 [`crate::new`] 配对产出，并注册进**同一个** Swarm。
#[derive(Debug)]
pub struct Transport {
    #[allow(
        dead_code,
        reason = "接后端时用于构造 PeerConnection 的 STUN / 超时配置"
    )]
    config: Config,
    channel: TransportSide,
    listeners: Vec<Listener>,
    pending: VecDeque<TransportEvent<BoxFuture<'static, Result<Output, Error>>, Error>>,
}

#[derive(Debug)]
struct Listener {
    id: ListenerId,
    addr: Multiaddr,
}

impl Transport {
    pub(crate) fn new(config: Config, channel: TransportSide) -> Self {
        Self {
            config,
            channel,
            listeners: Vec::new(),
            pending: VecDeque::new(),
        }
    }

    /// 找出能承接某个入站连接的监听器。
    ///
    /// 入站连接经 relay 而来，与具体监听地址无关，故取第一个仍在的监听器即可。
    fn listener_for_incoming(&self) -> Option<&Listener> {
        self.listeners.first()
    }
}

impl CoreTransport for Transport {
    type Output = Output;
    type Error = Error;
    type ListenerUpgrade = BoxFuture<'static, Result<Output, Error>>;
    type Dial = BoxFuture<'static, Result<Output, Error>>;

    /// 监听地址形如 `<relay-addr>/p2p-circuit/webrtc`——**不含**目标 `/p2p` 段
    /// （目标就是本机，由 swarm 在对外通告时补上）。
    fn listen_on(
        &mut self,
        id: ListenerId,
        addr: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        if !addr::is_webrtc(&addr) {
            return Err(TransportError::MultiaddrNotSupported(addr));
        }
        self.listeners.push(Listener {
            id,
            addr: addr.clone(),
        });
        self.pending.push_back(TransportEvent::NewAddress {
            listener_id: id,
            listen_addr: addr,
        });
        Ok(())
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        let Some(idx) = self.listeners.iter().position(|l| l.id == id) else {
            return false;
        };
        self.listeners.remove(idx);
        self.pending.push_back(TransportEvent::ListenerClosed {
            listener_id: id,
            reason: Ok(()),
        });
        true
    }

    /// 拨 `<relay-addr>/p2p-circuit/webrtc/p2p/<target>`。
    ///
    /// 本方法只负责把请求投递给 behaviour；真正的信令与打洞在那边完成。
    fn dial(
        &mut self,
        addr: Multiaddr,
        _opts: DialOpts,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
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
            let send_back_addr = addr::from_circuit(&local_addr, peer);
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
    use crate::channel;
    use futures::executor::block_on;
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

    /// webrtc-direct 是另一个传输——错认会把它的地址从官方实现手里抢走。
    #[test]
    fn rejects_foreign_addrs() {
        let (mut t, _b) = transport();
        for s in [
            "/ip4/1.2.3.4/tcp/4001",
            "/ip4/1.2.3.4/udp/4001/webrtc-direct",
        ] {
            let addr: Multiaddr = s.parse().unwrap();
            assert!(matches!(
                t.listen_on(ListenerId::next(), addr.clone()),
                Err(TransportError::MultiaddrNotSupported(_))
            ));
            assert!(matches!(
                t.dial(addr, dial_opts()),
                Err(TransportError::MultiaddrNotSupported(_))
            ));
        }
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
