//! [`NetworkBehaviour`] 实现：在 relay 连接上跑 `/webrtc-signaling/0.0.1`。
//!
//! 与 [`crate::Transport`] 配对工作——transport 收到 dial 请求后转交这里，因为「在已有
//! 连接上开协议流」只有 behaviour 做得到（原委见 [`crate::swarm::channel`]）。
//!
//! # 对称性
//!
//! 出入两个方向都必须走通，这是 spec 的 MUST 而非优化：
//!
//! - **出站**：收到 [`ToBehaviour::Dial`] → 确保有到对端的连接 → 下达
//!   [`Command::Start`] → handler 开流发 offer
//! - **入站**：对端开来信令流 → handler **自行受理**（不经本 behaviour 触发）→
//!   建连后经 [`ToTransport::Incoming`] 交给 transport
//!
//! 只做前者，本端就只能拨别人、不能被拨，覆盖矩阵里「web ↔ NAT 后原生端」那一格会塌。

use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll};

use futures::StreamExt;
use futures::channel::oneshot;
use libp2p_core::transport::PortUse;
use libp2p_core::{Endpoint, Multiaddr};
use libp2p_identity::PeerId;
use libp2p_swarm::{
    ConnectionDenied, ConnectionId, DialError, FromSwarm, NetworkBehaviour, NotifyHandler,
    THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    dial_opts::{DialOpts, PeerCondition},
};

use crate::backend::Factory;
use crate::config::Config;
use crate::error::Error;
use crate::swarm::channel::{BehaviourSide, ToBehaviour, ToTransport};
use crate::swarm::connection::Connection;
use crate::swarm::handler::{self, Handler};

/// behaviour 对外抛出的事件。
#[derive(Debug)]
pub enum Event {
    /// 与某节点的 WebRTC 直连已建立（打洞成功）。
    DirectConnectionEstablished { peer: PeerId },
    /// 信令或打洞失败。上层可据此决定是否继续用 relay 中转——
    /// spec 步骤 8 明确把这个回退策略留给应用。
    Failed { peer: PeerId, error: Error },
}

/// WebRTC 打洞的信令 behaviour。
///
/// 必须与 [`crate::Transport`] 由 [`crate::new`] 配对产出，并注册进**同一个** Swarm。
pub struct Behaviour {
    config: Config,
    factory: Factory,
    channel: BehaviourSide,
    /// 等待信令完成的拨号请求。
    ///
    /// 键是目标节点：spec 约定由发起方开流，同一对节点同时只跑一轮信令。
    pending_dials: HashMap<PeerId, oneshot::Sender<Result<Connection, Error>>>,
    /// 已有连接、但尚未下达 [`Command::Start`] 的目标。
    ///
    /// 信令要跑在**已建立**的连接上，故拨号请求到来时若还没连上，得先等
    /// `ConnectionEstablished` 再发命令。
    awaiting_connection: HashSet<PeerId>,
    connected: HashSet<PeerId>,
    queued: VecDeque<ToSwarm<Event, THandlerInEvent<Self>>>,
}

impl std::fmt::Debug for Behaviour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Behaviour")
            .field("pending_dials", &self.pending_dials.len())
            .field("awaiting_connection", &self.awaiting_connection.len())
            .field("connected", &self.connected.len())
            .field("queued", &self.queued.len())
            .finish_non_exhaustive()
    }
}

impl Behaviour {
    pub(crate) fn new(config: Config, factory: Factory, channel: BehaviourSide) -> Self {
        Self {
            config,
            factory,
            channel,
            pending_dials: HashMap::new(),
            awaiting_connection: HashSet::new(),
            connected: HashSet::new(),
            queued: VecDeque::new(),
        }
    }

    /// 处理来自 transport 的拨号请求。
    fn on_dial_request(
        &mut self,
        target: PeerId,
        signaling_addr: Multiaddr,
        result: oneshot::Sender<Result<Connection, Error>>,
    ) {
        if self.pending_dials.contains_key(&target) {
            // 同一对节点重复发起会建出两条 WebRTC 连接——spec 用「由 A 发起」的约定
            // 来避免它，这里再兜一道。
            let _ = result.send(Err(Error::SignalingAborted(
                "已有一轮信令在进行中".to_string(),
            )));
            return;
        }
        self.pending_dials.insert(target, result);

        if self.connected.contains(&target) {
            self.start_signaling(target);
            return;
        }

        // 尚未连上：先建 relay 连接，待 ConnectionEstablished 再下达命令。
        self.awaiting_connection.insert(target);
        self.queued.push_back(ToSwarm::Dial {
            opts: DialOpts::peer_id(target)
                // 已有连接就复用——信令不需要独占连接，多拨一条纯属浪费。
                .condition(PeerCondition::DisconnectedAndNotDialing)
                .addresses(vec![signaling_addr])
                .build(),
        });
    }

    fn start_signaling(&mut self, target: PeerId) {
        self.awaiting_connection.remove(&target);
        self.queued.push_back(ToSwarm::NotifyHandler {
            peer_id: target,
            handler: NotifyHandler::Any,
            event: handler::Command::Start,
        });
    }

    /// 结束一次拨号，把结果回送给 transport 的 dial future。
    fn finish_dial(&mut self, peer: PeerId, outcome: Result<Connection, Error>) {
        if let Some(tx) = self.pending_dials.remove(&peer) {
            let _ = tx.send(outcome);
        }
        self.awaiting_connection.remove(&peer);
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(self.config.clone(), self.factory.clone()))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(self.config.clone(), self.factory.clone()))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(e) => {
                self.connected.insert(e.peer_id);
                if self.awaiting_connection.contains(&e.peer_id) {
                    self.start_signaling(e.peer_id);
                }
            }
            FromSwarm::ConnectionClosed(e) => {
                if e.remaining_established == 0 {
                    self.connected.remove(&e.peer_id);
                    // 连接没了信令就无从继续，等超时不如立刻失败。
                    if self.pending_dials.contains_key(&e.peer_id) {
                        self.finish_dial(
                            e.peer_id,
                            Err(Error::SignalingAborted("信令所在连接已关闭".into())),
                        );
                    }
                }
            }
            FromSwarm::DialFailure(e) => {
                if let Some(peer) = e.peer_id
                    && self.awaiting_connection.contains(&peer)
                {
                    let reason = match e.error {
                        DialError::NoAddresses => "没有可用地址".to_string(),
                        other => other.to_string(),
                    };
                    self.finish_dial(
                        peer,
                        Err(Error::SignalingAborted(format!(
                            "建立信令连接失败：{reason}"
                        ))),
                    );
                }
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            handler::Event::Connected(muxer) => {
                let conn = Connection::new(peer, muxer);
                if self.pending_dials.contains_key(&peer) {
                    // 出站：回送给 transport 的 dial future。
                    self.finish_dial(peer, Ok(conn));
                } else {
                    // 入站：交给 transport 产出 TransportEvent::Incoming。
                    // 通道满说明 transport 侧堆积严重，丢弃并留痕好过阻塞 behaviour。
                    if let Err(e) = self.channel.to_transport.try_send(ToTransport::Incoming {
                        peer,
                        connection: conn,
                    }) {
                        tracing::warn!(%peer, "入站 WebRTC 连接无法交付 transport：{e}");
                    }
                }
                self.queued
                    .push_back(ToSwarm::GenerateEvent(Event::DirectConnectionEstablished {
                        peer,
                    }));
            }
            handler::Event::Failed(error) => {
                let report = Error::SignalingAborted(error.to_string());
                self.finish_dial(peer, Err(error));
                self.queued.push_back(ToSwarm::GenerateEvent(Event::Failed {
                    peer,
                    error: report,
                }));
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        loop {
            if let Some(event) = self.queued.pop_front() {
                return Poll::Ready(event);
            }
            match self.channel.from_transport.poll_next_unpin(cx) {
                Poll::Ready(Some(ToBehaviour::Dial {
                    target,
                    signaling_addr,
                    result,
                })) => self.on_dial_request(target, signaling_addr, result),
                // transport 已释放：不再有新的拨号请求，但入站方向仍需继续工作。
                Poll::Ready(None) | Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mock::MockBackend;
    use crate::swarm::channel;
    use futures::channel::oneshot;

    fn peer(seed: u8) -> PeerId {
        libp2p_identity::Keypair::ed25519_from_bytes([seed; 32])
            .unwrap()
            .public()
            .to_peer_id()
    }

    fn factory() -> Factory {
        std::sync::Arc::new(|_: &Config| {
            let (backend, _) = MockBackend::new([]);
            Ok(Box::new(backend) as Box<dyn crate::Backend>)
        })
    }

    fn behaviour() -> (Behaviour, channel::TransportSide) {
        let (t, b) = channel::pair();
        (Behaviour::new(Config::default(), factory(), b), t)
    }

    fn addr() -> Multiaddr {
        "/ip4/1.2.3.4/tcp/4001".parse().unwrap()
    }

    /// 空数据面：只为让 `Connected` 事件能被构造。真实数据面由集成测试覆盖。
    fn null_muxer() -> libp2p_core::muxing::StreamMuxerBox {
        use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
        use std::pin::Pin;

        struct NullStream;
        impl futures::AsyncRead for NullStream {
            fn poll_read(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
                _: &mut [u8],
            ) -> Poll<std::io::Result<usize>> {
                Poll::Ready(Ok(0))
            }
        }
        impl futures::AsyncWrite for NullStream {
            fn poll_write(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<std::io::Result<usize>> {
                Poll::Ready(Ok(buf.len()))
            }
            fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
                Poll::Ready(Ok(()))
            }
            fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
                Poll::Ready(Ok(()))
            }
        }

        struct NullMuxer;
        impl StreamMuxer for NullMuxer {
            type Substream = NullStream;
            type Error = std::io::Error;
            fn poll_inbound(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
            ) -> Poll<Result<Self::Substream, Self::Error>> {
                Poll::Pending
            }
            fn poll_outbound(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
            ) -> Poll<Result<Self::Substream, Self::Error>> {
                Poll::Pending
            }
            fn poll_close(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
            ) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
            fn poll(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
            ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
                Poll::Pending
            }
        }

        libp2p_core::muxing::StreamMuxerBox::new(NullMuxer)
    }

    /// 未连上时应先拨号，而不是直接给一个不存在的连接发命令。
    #[test]
    fn dial_request_without_connection_dials_first() {
        let (mut b, _t) = behaviour();
        let (tx, _rx) = oneshot::channel();
        b.on_dial_request(peer(1), addr(), tx);

        assert!(b.awaiting_connection.contains(&peer(1)));
        assert!(matches!(b.queued.front(), Some(ToSwarm::Dial { .. })));
    }

    /// 已连上则直接下达 Start，省掉一次多余拨号。
    #[test]
    fn dial_request_with_connection_starts_immediately() {
        let (mut b, _t) = behaviour();
        b.connected.insert(peer(1));
        let (tx, _rx) = oneshot::channel();
        b.on_dial_request(peer(1), addr(), tx);

        assert!(!b.awaiting_connection.contains(&peer(1)));
        assert!(matches!(
            b.queued.front(),
            Some(ToSwarm::NotifyHandler {
                event: handler::Command::Start,
                ..
            })
        ));
    }

    /// 重复发起会建出两条 WebRTC 连接，必须挡住。
    #[test]
    fn rejects_concurrent_dial_to_same_peer() {
        let (mut b, _t) = behaviour();
        let (tx1, _rx1) = oneshot::channel();
        b.on_dial_request(peer(1), addr(), tx1);

        let (tx2, mut rx2) = oneshot::channel();
        b.on_dial_request(peer(1), addr(), tx2);

        assert_eq!(b.pending_dials.len(), 1, "第二次不应覆盖第一次");
        assert!(
            matches!(rx2.try_recv(), Ok(Some(Err(Error::SignalingAborted(_))))),
            "第二次应立即失败而非静默丢弃"
        );
    }

    /// handler 报成功 → dial future 拿到连接。
    #[test]
    fn handler_connected_completes_pending_dial() {
        let (mut b, _t) = behaviour();
        let (tx, mut rx) = oneshot::channel();
        b.pending_dials.insert(peer(1), tx);

        b.on_connection_handler_event(
            peer(1),
            ConnectionId::new_unchecked(0),
            handler::Event::Connected(null_muxer()),
        );

        assert!(matches!(rx.try_recv(), Ok(Some(Ok(_)))));
        assert!(b.pending_dials.is_empty());
    }

    /// 入站方向：没有 pending_dial，连接应交给 transport 而非丢弃。
    #[test]
    fn inbound_connection_goes_to_transport() {
        let (mut b, mut t) = behaviour();
        b.on_connection_handler_event(
            peer(9),
            ConnectionId::new_unchecked(0),
            handler::Event::Connected(null_muxer()),
        );

        let expected = peer(9);
        let msg = futures::executor::block_on(t.from_behaviour.next());
        assert!(
            matches!(msg, Some(ToTransport::Incoming { peer, .. }) if peer == expected),
            "入站连接必须交给 transport，否则本端只能拨不能被拨"
        );
    }

    /// 连接断了信令无从继续，应立刻失败而不是等超时。
    #[test]
    fn connection_closed_fails_pending_dial() {
        let (mut b, _t) = behaviour();
        let (tx, mut rx) = oneshot::channel();
        b.pending_dials.insert(peer(1), tx);
        b.connected.insert(peer(1));

        b.on_swarm_event(FromSwarm::ConnectionClosed(
            libp2p_swarm::behaviour::ConnectionClosed {
                peer_id: peer(1),
                connection_id: ConnectionId::new_unchecked(0),
                endpoint: &libp2p_core::ConnectedPoint::Dialer {
                    address: addr(),
                    role_override: Endpoint::Dialer,
                    port_use: PortUse::Reuse,
                },
                remaining_established: 0,
                cause: None,
            },
        ));

        assert!(matches!(
            rx.try_recv(),
            Ok(Some(Err(Error::SignalingAborted(_))))
        ));
        assert!(!b.connected.contains(&peer(1)));
    }
}
