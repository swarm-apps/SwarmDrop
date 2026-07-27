//! 信令状态机（[`ConnectionHandler`]）。
//!
//! 一条 relay 连接配一个 handler，在其上跑 `/webrtc-signaling/0.0.1`。
//!
//! # 两个角色对称走通
//!
//! spec 步骤 4 是 MUST：每一侧都要能 offer 也能 answer。「由 A 发起」只是防止双方同时
//! 发起而建出两条连接的**约定**，不是能力划分。
//!
//! | | 触发 | 首个动作 | 随后 |
//! |---|---|---|---|
//! | 发起方 | behaviour 下达 [`Command::Start`] | 请求开出站流 → `start_offer` | 收 answer → 交换候选 |
//! | 应答方 | 对端开来入站流 | 等 offer → `accept_offer` | 发 answer → 交换候选 |
//!
//! 应答方**不需要** behaviour 通知——入站流到达即自行受理。这正是「能被拨」的实现。
//!
//! # 泵的顺序
//!
//! `poll` 每轮按「后端 → 发送队列 → 信令流 → 后端」的方向搬运，任一环推进即返回，
//! 避免在单次 poll 里空转。

use std::collections::VecDeque;
use std::task::{Context, Poll};

use asynchronous_codec::Framed;
use futures::{SinkExt, StreamExt};
use libp2p_core::upgrade::ReadyUpgrade;
use libp2p_swarm::handler::{
    ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent, FullyNegotiatedInbound,
    FullyNegotiatedOutbound, SubstreamProtocol,
};
use libp2p_swarm::{Stream, StreamProtocol};

use crate::backend::{Backend, BackendError, BackendEvent, Factory};
use crate::codec::Codec;
use crate::signaling::{Message, MessageType, SIGNALING_PROTOCOL};
use crate::{Config, Error};

/// 供 `SubstreamProtocol` 使用的协议标识。
pub(crate) fn protocol() -> StreamProtocol {
    StreamProtocol::new(SIGNALING_PROTOCOL)
}

/// behaviour → handler。
#[derive(Debug)]
pub enum Command {
    /// 作为发起方开始信令。
    Start,
}

/// handler → behaviour。
#[derive(Debug)]
pub enum Event {
    /// 直连建立（spec 步骤 8）。
    Connected,
    /// 信令或建连失败。
    Failed(Error),
}

/// 本端在这轮信令中的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Initiator,
    Responder,
}

/// 信令状态机。
pub struct Handler {
    config: Config,
    factory: Factory,
    role: Option<Role>,
    backend: Option<Box<dyn Backend>>,
    stream: Option<Framed<Stream, Codec>>,
    /// 待发往对端的消息。
    outbox: VecDeque<Message>,
    /// 待报给 behaviour 的事件。
    events: VecDeque<Event>,
    /// 已请求过出站流，避免重复请求。
    requested_outbound: bool,
    /// 终态：不再收发，也不再请求新流。
    finished: bool,
}

impl std::fmt::Debug for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handler")
            .field("role", &self.role)
            .field("has_backend", &self.backend.is_some())
            .field("has_stream", &self.stream.is_some())
            .field("outbox", &self.outbox.len())
            .field("finished", &self.finished)
            .finish()
    }
}

impl Handler {
    pub(crate) fn new(config: Config, factory: Factory) -> Self {
        Self {
            config,
            factory,
            role: None,
            backend: None,
            stream: None,
            outbox: VecDeque::new(),
            events: VecDeque::new(),
            requested_outbound: false,
            finished: false,
        }
    }

    /// 进入终态并上报原因。后续 poll 不再做任何 IO。
    fn fail(&mut self, error: Error) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.stream = None;
        self.backend = None;
        self.events.push_back(Event::Failed(error));
    }

    fn ensure_backend(&mut self) -> Result<(), BackendError> {
        if self.backend.is_none() {
            self.backend = Some((self.factory)(&self.config)?);
        }
        Ok(())
    }

    /// 收到对端信令消息。
    fn on_message(&mut self, msg: Message) {
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let Some(data) = msg.data.as_deref() else {
            // 有类型无数据：对端实现有问题，但不值得因此断连——记录后忽略。
            tracing::debug!(?msg.ty, "信令消息缺少 data 字段，忽略");
            return;
        };
        let result = match msg.ty {
            Some(MessageType::SdpOffer) => backend.accept_offer(data),
            Some(MessageType::SdpAnswer) => backend.accept_answer(data),
            Some(MessageType::IceCandidate) => backend.add_remote_candidate(data),
            None => {
                tracing::debug!("信令消息缺少 type 字段，忽略");
                return;
            }
        };
        if let Err(e) = result {
            self.fail(Error::Connection(e.to_string()));
        }
    }

    /// 后端 → 发送队列 / 事件。返回是否有推进。
    fn pump_backend(&mut self, cx: &mut Context<'_>) -> bool {
        let Some(backend) = self.backend.as_mut() else {
            return false;
        };
        let mut progressed = false;
        while let Poll::Ready(event) = backend.poll(cx) {
            progressed = true;
            match event {
                BackendEvent::LocalDescription { ty, sdp } => {
                    self.outbox.push_back(Message {
                        ty: Some(ty),
                        data: Some(sdp),
                    });
                }
                BackendEvent::LocalCandidate(json) => {
                    self.outbox.push_back(Message::ice_candidate(json));
                }
                BackendEvent::Connected => {
                    // spec 步骤 8：成功后关闭信令流（init DataChannel 由后端自行关闭）。
                    self.finished = true;
                    self.stream = None;
                    self.events.push_back(Event::Connected);
                    return true;
                }
                BackendEvent::Failed(msg) => {
                    self.fail(Error::Connection(msg));
                    return true;
                }
            }
        }
        progressed
    }

    /// 发送队列 → 信令流。返回是否有推进。
    fn pump_outbox(&mut self, cx: &mut Context<'_>) -> bool {
        let Some(stream) = self.stream.as_mut() else {
            return false;
        };
        let mut progressed = false;
        while !self.outbox.is_empty() {
            match stream.poll_ready_unpin(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => {
                    self.fail(Error::Signaling(e));
                    return true;
                }
                Poll::Pending => break,
            }
            let msg = self.outbox.pop_front().expect("非空");
            if let Err(e) = stream.start_send_unpin(msg) {
                self.fail(Error::Signaling(e));
                return true;
            }
            progressed = true;
        }
        if let Some(stream) = self.stream.as_mut()
            && let Poll::Ready(Err(e)) = stream.poll_flush_unpin(cx)
        {
            self.fail(Error::Signaling(e));
            return true;
        }
        progressed
    }

    /// 信令流 → 后端。返回是否有推进。
    fn pump_stream(&mut self, cx: &mut Context<'_>) -> bool {
        let mut progressed = false;
        loop {
            let Some(stream) = self.stream.as_mut() else {
                return progressed;
            };
            match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(msg))) => {
                    progressed = true;
                    self.on_message(msg);
                }
                Poll::Ready(Some(Err(e))) => {
                    self.fail(Error::Signaling(e));
                    return true;
                }
                Poll::Ready(None) => {
                    // 对端关流。若尚未建连，视为中止；已建连则是 spec 步骤 8 的正常收尾。
                    if !self.finished {
                        self.fail(Error::SignalingAborted("对端关闭了信令流".into()));
                    }
                    return true;
                }
                Poll::Pending => return progressed,
            }
        }
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = Command;
    type ToBehaviour = Event;
    type InboundProtocol = ReadyUpgrade<StreamProtocol>;
    type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, ()> {
        SubstreamProtocol::new(ReadyUpgrade::new(protocol()), ())
    }

    fn connection_keep_alive(&self) -> bool {
        // 信令未完成前必须保住 relay 连接——它断了 SDP 就换不完。
        !self.finished
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            Command::Start => {
                if self.role.is_some() || self.finished {
                    return;
                }
                self.role = Some(Role::Initiator);
            }
        }
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        match event {
            // 对端开来信令流 —— 应答方路径，无需 behaviour 通知即自行受理。
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: stream,
                ..
            }) => {
                if self.finished {
                    return;
                }
                self.role.get_or_insert(Role::Responder);
                if let Err(e) = self.ensure_backend() {
                    self.fail(Error::Connection(e.to_string()));
                    return;
                }
                self.stream = Some(Framed::new(stream, Codec));
            }
            // 我们请求的出站流就绪 —— 发起方路径，可以造 offer 了。
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                ..
            }) => {
                if self.finished {
                    return;
                }
                if let Err(e) = self.ensure_backend() {
                    self.fail(Error::Connection(e.to_string()));
                    return;
                }
                self.stream = Some(Framed::new(stream, Codec));
                if let Some(backend) = self.backend.as_mut()
                    && let Err(e) = backend.start_offer()
                {
                    self.fail(Error::Connection(e.to_string()));
                }
            }
            ConnectionEvent::DialUpgradeError(err) => {
                self.fail(Error::SignalingAborted(format!("开流失败：{}", err.error)));
            }
            ConnectionEvent::ListenUpgradeError(err) => {
                self.fail(Error::SignalingAborted(format!(
                    "受理入站流失败：{}",
                    err.error
                )));
            }
            _ => {}
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
            }
            if self.finished {
                return Poll::Pending;
            }

            // 发起方要先有一条出站流才能发 offer。
            if self.role == Some(Role::Initiator)
                && self.stream.is_none()
                && !self.requested_outbound
            {
                self.requested_outbound = true;
                return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(ReadyUpgrade::new(protocol()), ()),
                });
            }

            let progressed = self.pump_backend(cx) | self.pump_outbox(cx) | self.pump_stream(cx);
            if !progressed {
                return Poll::Pending;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mock::{Calls, MockBackend};
    use std::sync::{Arc, Mutex};
    use std::task::Waker;

    fn noop_factory() -> Factory {
        Arc::new(|_: &Config| {
            let (b, _) = MockBackend::new([]);
            Ok(Box::new(b) as Box<dyn Backend>)
        })
    }

    /// 构造一个已装好 mock 后端的 handler（跳过开流，聚焦状态机本身）。
    fn handler_with(
        events: impl IntoIterator<Item = BackendEvent>,
    ) -> (Handler, Arc<Mutex<Calls>>) {
        let mut h = Handler::new(Config::default(), noop_factory());
        let (backend, calls) = MockBackend::new(events);
        h.backend = Some(Box::new(backend));
        (h, calls)
    }

    fn poll_once(
        h: &mut Handler,
    ) -> Poll<ConnectionHandlerEvent<ReadyUpgrade<StreamProtocol>, (), Event>> {
        let mut cx = Context::from_waker(Waker::noop());
        h.poll(&mut cx)
    }

    #[test]
    fn start_command_makes_us_initiator_and_requests_stream() {
        let mut h = Handler::new(Config::default(), noop_factory());
        h.on_behaviour_event(Command::Start);
        assert_eq!(h.role, Some(Role::Initiator));

        assert!(matches!(
            poll_once(&mut h),
            Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest { .. })
        ));
        // 再 poll 不应重复请求——否则每轮都会开一条新流。
        assert!(matches!(poll_once(&mut h), Poll::Pending));
    }

    /// 应答方不需要 behaviour 通知，也不该主动开流（spec 约定由 A 发起）。
    #[test]
    fn responder_never_requests_outbound_stream() {
        let (mut h, _) = handler_with([]);
        h.role = Some(Role::Responder);
        assert!(matches!(poll_once(&mut h), Poll::Pending));
        assert!(!h.requested_outbound);
    }

    #[test]
    fn local_sdp_and_candidates_are_queued_for_sending() {
        let (mut h, _) = handler_with([
            BackendEvent::LocalDescription {
                ty: MessageType::SdpOffer,
                sdp: "v=0".into(),
            },
            BackendEvent::LocalCandidate("cand-1".into()),
            BackendEvent::LocalCandidate("cand-2".into()),
        ]);

        let _ = poll_once(&mut h);

        // 无流可写时应留在队列里，等流就绪再发——而不是丢弃。
        assert_eq!(h.outbox.len(), 3);
        assert_eq!(h.outbox[0], Message::offer("v=0"));
        assert_eq!(h.outbox[1], Message::ice_candidate("cand-1"));
    }

    #[test]
    fn connected_finishes_and_notifies() {
        let (mut h, _) = handler_with([BackendEvent::Connected]);

        assert!(matches!(
            poll_once(&mut h),
            Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Event::Connected))
        ));
        assert!(h.finished);
        // spec 步骤 8：成功后关闭信令流。
        assert!(h.stream.is_none());
        // keep-alive 必须放开，否则 relay 连接会被白白占住。
        assert!(!h.connection_keep_alive());
    }

    #[test]
    fn backend_failure_is_reported_once() {
        let (mut h, _) = handler_with([BackendEvent::Failed("boom".into())]);

        assert!(matches!(
            poll_once(&mut h),
            Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Event::Failed(_)))
        ));
        assert!(h.finished);
        assert!(matches!(poll_once(&mut h), Poll::Pending), "不应重复上报");
    }

    /// 三种消息各自分派到正确的后端方法——错位会导致 offer 被当 answer 处理。
    #[test]
    fn messages_dispatch_to_matching_backend_call() {
        let (mut h, calls) = handler_with([]);
        h.on_message(Message::offer("o"));
        h.on_message(Message::answer("a"));
        h.on_message(Message::ice_candidate("c"));

        let calls = calls.lock().unwrap();
        assert_eq!(calls.accepted_offer, ["o"]);
        assert_eq!(calls.accepted_answer, ["a"]);
        assert_eq!(calls.remote_candidates, ["c"]);
    }

    /// 字段缺失的消息只忽略，不应拖垮整条信令——对端实现可能更宽松。
    #[test]
    fn malformed_messages_are_ignored_not_fatal() {
        let (mut h, calls) = handler_with([]);
        h.on_message(Message {
            ty: Some(MessageType::SdpOffer),
            data: None,
        });
        h.on_message(Message {
            ty: None,
            data: Some("x".into()),
        });

        assert!(!h.finished, "不应因此终止信令");
        let calls = calls.lock().unwrap();
        assert!(calls.accepted_offer.is_empty());
    }

    #[test]
    fn fail_is_idempotent() {
        let (mut h, _) = handler_with([]);
        h.fail(Error::SignalingAborted("first".into()));
        h.fail(Error::SignalingAborted("second".into()));
        assert_eq!(h.events.len(), 1, "重复失败只上报一次");
    }
}
