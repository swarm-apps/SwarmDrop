//! 按 peer 白名单维持连接的 keep-alive behaviour（迁自旧栈，逻辑不变）。
//!
//! libp2p 0.52+ 移除了 ping 的 keep-alive 语义：只要所有 ConnectionHandler 的
//! `connection_keep_alive()` 返回 false 且无活跃流，连接就会在
//! `idle_connection_timeout` 后被 swarm 以 KeepAliveTimeout 关闭。
//!
//! 本 behaviour 提供逐 peer 的保活豁免：白名单内 peer 的 handler 返回
//! keep_alive=true，连接永不因空闲被回收；白名单外行为不变。
//! 白名单由上层业务（如「已配对设备」）经 `Endpoint::set_keep_alive` 维护。

use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::task::{Context, Poll, Waker};

use libp2p::PeerId;
use libp2p::core::transport::PortUse;
use libp2p::core::upgrade::DeniedUpgrade;
use libp2p::core::{Endpoint, Multiaddr};
use libp2p::swarm::handler::{
    ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
};
use libp2p::swarm::{
    ConnectionDenied, ConnectionHandlerEvent, ConnectionId, FromSwarm, NetworkBehaviour,
    NotifyHandler, StreamUpgradeError, SubstreamProtocol, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};

/// behaviour → handler 的控制事件
#[derive(Debug)]
pub enum HandlerIn {
    /// 更新该连接的保活标志
    SetKeepAlive(bool),
}

/// 逐 peer 保活白名单 behaviour
#[derive(Default)]
pub struct Behaviour {
    /// 保活白名单
    allowed: HashSet<PeerId>,
    /// peer → 活跃连接，白名单变更时用于通知既有连接的 handler
    connections: HashMap<PeerId, Vec<ConnectionId>>,
    /// 待下发给 handler 的通知
    pending: VecDeque<ToSwarm<Infallible, HandlerIn>>,
    waker: Option<Waker>,
}

impl Behaviour {
    /// 增删白名单，并同步通知该 peer 所有既有连接的 handler
    pub fn set_keep_alive(&mut self, peer_id: PeerId, enabled: bool) {
        let changed = if enabled {
            self.allowed.insert(peer_id)
        } else {
            self.allowed.remove(&peer_id)
        };
        if !changed {
            return;
        }
        for connection_id in self.connections.get(&peer_id).into_iter().flatten() {
            self.pending.push_back(ToSwarm::NotifyHandler {
                peer_id,
                handler: NotifyHandler::One(*connection_id),
                event: HandlerIn::SetKeepAlive(enabled),
            });
        }
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    fn on_established(&mut self, connection_id: ConnectionId, peer_id: PeerId) -> Handler {
        self.connections
            .entry(peer_id)
            .or_default()
            .push(connection_id);
        Handler {
            keep_alive: self.allowed.contains(&peer_id),
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.on_established(connection_id, peer_id))
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.on_established(connection_id, peer_id))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(closed) = event
            && let Some(ids) = self.connections.get_mut(&closed.peer_id)
        {
            ids.retain(|id| *id != closed.connection_id);
            if ids.is_empty() {
                self.connections.remove(&closed.peer_id);
            }
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        libp2p::core::util::unreachable(event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.pending.pop_front() {
            return Poll::Ready(event);
        }
        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

/// 不承载任何协议、只回答 keep-alive 询问的 handler
pub struct Handler {
    keep_alive: bool,
}

impl libp2p::swarm::ConnectionHandler for Handler {
    type FromBehaviour = HandlerIn;
    type ToBehaviour = Infallible;
    type InboundProtocol = DeniedUpgrade;
    type OutboundProtocol = DeniedUpgrade;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        SubstreamProtocol::new(DeniedUpgrade, ())
    }

    fn connection_keep_alive(&self) -> bool {
        self.keep_alive
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            HandlerIn::SetKeepAlive(enabled) => self.keep_alive = enabled,
        }
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        Poll::Pending
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol, ..
            }) => libp2p::core::util::unreachable(protocol),
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol, ..
            }) => libp2p::core::util::unreachable(protocol),
            ConnectionEvent::DialUpgradeError(DialUpgradeError { info: _, error }) => match error {
                StreamUpgradeError::Timeout => unreachable!(),
                StreamUpgradeError::Apply(e) => libp2p::core::util::unreachable(e),
                StreamUpgradeError::NegotiationFailed | StreamUpgradeError::Io(_) => {
                    unreachable!("DeniedUpgrade 不协商任何协议")
                }
            },
            _ => {}
        }
    }
}
