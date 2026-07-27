//! [`NetworkBehaviour`] 实现：在 relay 连接上跑 `/webrtc-signaling/0.0.1`。
//!
//! 与 [`crate::Transport`] 配对工作——transport 收到 dial 请求后转交这里，因为「在已有
//! 连接上开协议流」只有 behaviour 做得到（原委见 [`crate::channel`]）。
//!
//! # 对称性
//!
//! 出入两个方向都必须走通，这是 spec 的 MUST 而非优化：
//!
//! - **出站**：收到 [`ToBehaviour::Dial`] → 确保有到对端的连接 → 开信令流 → 发 offer
//! - **入站**：对端开来信令流 → 回 answer → 建连后经 [`ToTransport::Incoming`] 交给 transport
//!
//! 只做前者，本端就只能拨别人、不能被拨，覆盖矩阵里「web ↔ NAT 后原生端」那一格会塌。

use std::collections::{HashMap, VecDeque};
use std::task::{Context, Poll};

use futures::StreamExt;
use futures::channel::oneshot;
use libp2p_core::transport::PortUse;
use libp2p_core::{Endpoint, Multiaddr};
use libp2p_identity::PeerId;
use libp2p_swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm, dummy,
};

use crate::channel::{BehaviourSide, ToBehaviour};
use crate::error::Error;
use crate::{Config, Connection};

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
    #[allow(
        dead_code,
        reason = "接后端时用于构造 PeerConnection 的 STUN / 超时配置"
    )]
    config: Config,
    channel: BehaviourSide,
    /// 已投递给 swarm、等待连接建立的拨号请求。
    ///
    /// 键是目标节点：spec 约定由发起方开流，同一对节点同时只跑一轮信令。
    pending_dials: HashMap<PeerId, oneshot::Sender<Result<Connection, Error>>>,
    queued: VecDeque<ToSwarm<Event, THandlerInEvent<Self>>>,
}

impl std::fmt::Debug for Behaviour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Behaviour")
            .field("pending_dials", &self.pending_dials.len())
            .field("queued", &self.queued.len())
            .finish_non_exhaustive()
    }
}

impl Behaviour {
    pub(crate) fn new(config: Config, channel: BehaviourSide) -> Self {
        Self {
            config,
            channel,
            pending_dials: HashMap::new(),
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

        // TODO(信令)：确保存在到 target 的连接后开 `/webrtc-signaling/0.0.1` 流并发 offer。
        // 现在只把拨号意图交给 swarm；handler 接上后在 on_connection_handler_event 里推进。
        tracing::debug!(%target, %signaling_addr, "收到 WebRTC 拨号请求（信令尚未接线）");
    }
}

impl NetworkBehaviour for Behaviour {
    // TODO(信令)：换成跑信令状态机的真 handler。dummy 期间入站流不会被受理，
    // 因此「入站」那半条路尚未打通——对称性要在 handler 落地时一并补齐。
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {
        // TODO(信令)：关注 ConnectionEstablished / DialFailure，用以推进或失败化
        // pending_dials 中的请求。
    }

    fn on_connection_handler_event(
        &mut self,
        _peer: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        // dummy handler 的事件类型是 Infallible，永不产生。
        libp2p_core::util::unreachable(event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.queued.pop_front() {
            return Poll::Ready(event);
        }

        while let Poll::Ready(Some(msg)) = self.channel.from_transport.poll_next_unpin(cx) {
            let ToBehaviour::Dial {
                target,
                signaling_addr,
                result,
            } = msg;
            self.on_dial_request(target, signaling_addr, result);
            if let Some(event) = self.queued.pop_front() {
                return Poll::Ready(event);
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel;
    use futures::channel::oneshot;

    fn peer(seed: u8) -> PeerId {
        libp2p_identity::Keypair::ed25519_from_bytes([seed; 32])
            .unwrap()
            .public()
            .to_peer_id()
    }

    fn behaviour() -> (Behaviour, channel::TransportSide) {
        let (t, b) = channel::pair();
        (Behaviour::new(Config::default(), b), t)
    }

    #[test]
    fn tracks_pending_dial() {
        let (mut b, _t) = behaviour();
        let (tx, _rx) = oneshot::channel();
        b.on_dial_request(peer(1), "/ip4/1.2.3.4/tcp/4001".parse().unwrap(), tx);
        assert_eq!(b.pending_dials.len(), 1);
    }

    /// 重复发起会建出两条 WebRTC 连接，必须挡住。
    #[test]
    fn rejects_concurrent_dial_to_same_peer() {
        let (mut b, _t) = behaviour();
        let target = peer(1);
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse().unwrap();

        let (tx1, _rx1) = oneshot::channel();
        b.on_dial_request(target, addr.clone(), tx1);

        let (tx2, mut rx2) = oneshot::channel();
        b.on_dial_request(target, addr, tx2);

        assert_eq!(b.pending_dials.len(), 1, "第二次不应覆盖第一次");
        assert!(
            matches!(rx2.try_recv(), Ok(Some(Err(Error::SignalingAborted(_))))),
            "第二次应立即失败而非静默丢弃"
        );
    }

    #[test]
    fn different_peers_dial_concurrently() {
        let (mut b, _t) = behaviour();
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse().unwrap();
        for seed in [1, 2, 3] {
            let (tx, _rx) = oneshot::channel();
            b.on_dial_request(peer(seed), addr.clone(), tx);
        }
        assert_eq!(b.pending_dials.len(), 3);
    }
}
