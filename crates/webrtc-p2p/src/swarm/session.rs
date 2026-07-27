//! 信令会话状态机。
//!
//! **纯逻辑，不碰 IO，不依赖 `libp2p-swarm`**：吃「命令 / 对端消息 / 后端事件」，吐
//! 「宿主该做什么」（[`Action`]）。真正的读写由 [`super::handler`] 完成。
//!
//! 这样切开是因为状态机是本 crate 最容易出错的部分——摘出来后可以用同步方式逐步驱动、
//! 逐条断言，不必构造真实的 `Stream`，也不必进 poll 模型。
//!
//! # 角色对称
//!
//! spec 步骤 4 是 MUST：每一侧都要能 offer 也能 answer。「由 A 发起」只是防止双方同时
//! 发起而建出两条连接的约定，不是能力划分。
//!
//! | 角色 | 由谁触发 | 首个动作 |
//! |---|---|---|
//! | [`Role::Initiator`] | behaviour 下达命令 | [`Action::OpenStream`] → `start_offer` |
//! | [`Role::Responder`] | 对端开来入站流 | 等 offer → `accept_offer` |

use std::collections::VecDeque;
use std::task::{Context, Poll};

use crate::backend::{Backend, BackendEvent, Factory};
use crate::config::Config;
use crate::error::Error;
use crate::protocol::{Message, MessageType};

/// 本端在这轮信令中的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// 主动发起：开出站流并送出 offer。
    Initiator,
    /// 被动应答：受理入站流并回 answer。
    Responder,
}

/// 会话要求宿主执行的动作。
#[derive(Debug)]
pub enum Action {
    /// 需要一条出站信令流（仅发起方）。
    OpenStream,
    /// 直连已建立（spec 步骤 8）。
    Connected,
    /// 会话失败。
    Failed(Error),
}

/// 会话生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// 角色未定，尚未开始。
    Idle,
    /// 信令进行中。
    Signaling,
    /// 已终结（成功或失败），不再产生任何动作。
    Finished,
}

/// 信令会话。
pub struct Session {
    config: Config,
    factory: Factory,
    state: State,
    role: Option<Role>,
    backend: Option<Box<dyn Backend>>,
    /// 待发往对端的消息。宿主在流可写时用 [`Session::next_outgoing`] 取。
    ///
    /// 由会话而非宿主持有：候选是**陆续**产生的，流未就绪时必须攒着而不是丢弃——
    /// trickle ICE 少一条候选就可能少一条可用路径。
    outbox: VecDeque<Message>,
    /// 已要求宿主开过出站流，避免每轮 poll 重复要求。
    stream_requested: bool,
    stream_attached: bool,
    /// 待宿主取走的终结动作（Connected / Failed）。
    ///
    /// `fail()` 可能在 poll 之外被调用（开流失败、流读写出错），故动作要先存起来，
    /// 等下一次 poll 交出去。
    pending_action: Option<Action>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("state", &self.state)
            .field("role", &self.role)
            .field("has_backend", &self.backend.is_some())
            .field("outbox", &self.outbox.len())
            .field("stream_attached", &self.stream_attached)
            .finish()
    }
}

impl Session {
    pub fn new(config: Config, factory: Factory) -> Self {
        Self {
            config,
            factory,
            state: State::Idle,
            role: None,
            backend: None,
            outbox: VecDeque::new(),
            stream_requested: false,
            stream_attached: false,
            pending_action: None,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.state == State::Finished
    }

    #[cfg(test)]
    pub(crate) fn role(&self) -> Option<Role> {
        self.role
    }

    /// 作为发起方开始（behaviour 下达命令时调用）。
    ///
    /// 已有角色或已终结时静默忽略——重复下达不应打断进行中的会话。
    pub fn start_as_initiator(&mut self) {
        if self.state != State::Idle || self.role.is_some() {
            return;
        }
        self.role = Some(Role::Initiator);
        self.state = State::Signaling;
    }

    /// 信令流就绪。
    ///
    /// 入站流会顺带把角色定为应答方——**这一步不需要 behaviour 参与**，正是「能被拨」
    /// 的实现所在。
    pub fn attach_stream(&mut self, role: Role) {
        if self.state == State::Finished || self.stream_attached {
            return;
        }
        self.role.get_or_insert(role);
        self.state = State::Signaling;
        self.stream_attached = true;

        if let Err(e) = self.ensure_backend() {
            self.fail(e);
            return;
        }
        // 发起方在流就绪后立刻造 offer；应答方等对端的 offer 到来。
        if self.role == Some(Role::Initiator)
            && let Some(backend) = self.backend.as_mut()
            && let Err(e) = backend.start_offer()
        {
            self.fail(Error::Connection(e.to_string()));
        }
    }

    /// 收到对端信令消息。
    pub fn on_message(&mut self, msg: Message) {
        if self.state == State::Finished {
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let Some(data) = msg.data.as_deref() else {
            // 有类型无数据：对端实现有瑕疵，但不值得为此断连。
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

    /// 对端关闭了信令流。
    ///
    /// 已建连时这是 spec 步骤 8 的正常收尾；否则视为中止。
    pub fn on_stream_closed(&mut self) {
        if self.state != State::Finished {
            self.fail(Error::SignalingAborted("对端关闭了信令流".into()));
        }
    }

    /// 外部错误（开流失败、流读写出错等）。幂等。
    pub fn fail(&mut self, error: Error) {
        if self.state == State::Finished {
            return;
        }
        self.state = State::Finished;
        self.backend = None;
        self.outbox.clear();
        self.pending_action = Some(Action::Failed(error));
    }

    /// 取下一条待发消息。宿主应在流可写时反复调用直至返回 `None`。
    pub fn next_outgoing(&mut self) -> Option<Message> {
        self.outbox.pop_front()
    }

    /// 推进会话，取下一个动作。
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Action> {
        if let Some(action) = self.pending_action.take() {
            return Poll::Ready(action);
        }
        if self.state == State::Finished {
            return Poll::Pending;
        }

        // 发起方要先有一条出站流才能送 offer。
        if self.role == Some(Role::Initiator) && !self.stream_attached && !self.stream_requested {
            self.stream_requested = true;
            return Poll::Ready(Action::OpenStream);
        }

        let Some(backend) = self.backend.as_mut() else {
            return Poll::Pending;
        };
        while let Poll::Ready(event) = backend.poll(cx) {
            match event {
                BackendEvent::LocalDescription { ty, sdp } => self.outbox.push_back(Message {
                    ty: Some(ty),
                    data: Some(sdp),
                }),
                BackendEvent::LocalCandidate(json) => {
                    self.outbox.push_back(Message::ice_candidate(json))
                }
                BackendEvent::Connected => {
                    self.state = State::Finished;
                    return Poll::Ready(Action::Connected);
                }
                BackendEvent::Failed(msg) => {
                    self.fail(Error::Connection(msg));
                    return Poll::Ready(self.pending_action.take().expect("fail 必然置入动作"));
                }
            }
        }
        Poll::Pending
    }

    fn ensure_backend(&mut self) -> Result<(), Error> {
        if self.backend.is_none() {
            self.backend =
                Some((self.factory)(&self.config).map_err(|e| Error::Connection(e.to_string()))?);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendError;
    use crate::backend::mock::{Calls, MockBackend};
    use std::sync::{Arc, Mutex};
    use std::task::Waker;

    /// 造一个会话，其 factory 交出预设好脚本的 mock 后端。
    fn session_with(
        events: impl IntoIterator<Item = BackendEvent>,
    ) -> (Session, Arc<Mutex<Calls>>) {
        let (backend, calls) = MockBackend::new(events);
        let slot = Mutex::new(Some(backend));
        let factory: Factory = Arc::new(move |_: &Config| {
            slot.lock()
                .unwrap()
                .take()
                .map(|b| Box::new(b) as Box<dyn Backend>)
                .ok_or_else(|| BackendError::new("后端已被取走"))
        });
        (Session::new(Config::default(), factory), calls)
    }

    fn poll(s: &mut Session) -> Poll<Action> {
        s.poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn initiator_requests_stream_exactly_once() {
        let (mut s, _) = session_with([]);
        s.start_as_initiator();
        assert_eq!(s.role(), Some(Role::Initiator));

        assert!(matches!(poll(&mut s), Poll::Ready(Action::OpenStream)));
        // 再 poll 不应重复要求——否则每轮都会开一条新流。
        assert!(matches!(poll(&mut s), Poll::Pending));
    }

    /// 应答方不主动开流（spec 约定由 A 发起，双方同时发起会建出两条连接）。
    #[test]
    fn responder_never_requests_stream() {
        let (mut s, _) = session_with([]);
        s.attach_stream(Role::Responder);
        assert_eq!(s.role(), Some(Role::Responder));
        assert!(matches!(poll(&mut s), Poll::Pending));
    }

    /// 入站流可以在没有任何 behaviour 指令的情况下把会话带起来——这就是「能被拨」。
    #[test]
    fn inbound_stream_alone_starts_session() {
        let (mut s, calls) = session_with([]);
        s.attach_stream(Role::Responder);
        assert!(!calls.lock().unwrap().started_offer, "应答方不该造 offer");
        s.on_message(Message::offer("v=0"));
        assert_eq!(calls.lock().unwrap().accepted_offer, ["v=0"]);
    }

    #[test]
    fn initiator_creates_offer_when_stream_ready() {
        let (mut s, calls) = session_with([]);
        s.start_as_initiator();
        s.attach_stream(Role::Initiator);
        assert!(calls.lock().unwrap().started_offer);
    }

    /// 流未就绪时候选必须攒着——trickle ICE 丢一条候选就可能少一条可用路径。
    #[test]
    fn local_messages_are_queued_until_taken() {
        let (mut s, _) = session_with([
            BackendEvent::LocalDescription {
                ty: MessageType::SdpOffer,
                sdp: "v=0".into(),
            },
            BackendEvent::LocalCandidate("cand-1".into()),
            BackendEvent::LocalCandidate("cand-2".into()),
        ]);
        s.attach_stream(Role::Responder);
        let _ = poll(&mut s);

        assert_eq!(s.next_outgoing(), Some(Message::offer("v=0")));
        assert_eq!(s.next_outgoing(), Some(Message::ice_candidate("cand-1")));
        assert_eq!(s.next_outgoing(), Some(Message::ice_candidate("cand-2")));
        assert_eq!(s.next_outgoing(), None);
    }

    #[test]
    fn connected_finishes_session() {
        let (mut s, _) = session_with([BackendEvent::Connected]);
        s.attach_stream(Role::Responder);

        assert!(matches!(poll(&mut s), Poll::Ready(Action::Connected)));
        assert!(s.is_finished());
        assert!(matches!(poll(&mut s), Poll::Pending), "终结后不再产生动作");
    }

    #[test]
    fn backend_failure_reported_once() {
        let (mut s, _) = session_with([BackendEvent::Failed("boom".into())]);
        s.attach_stream(Role::Responder);

        assert!(matches!(poll(&mut s), Poll::Ready(Action::Failed(_))));
        assert!(s.is_finished());
        assert!(matches!(poll(&mut s), Poll::Pending));
    }

    /// 三种消息各自分派到正确的后端方法——错位会让 offer 被当成 answer 处理。
    #[test]
    fn messages_dispatch_to_matching_call() {
        let (mut s, calls) = session_with([]);
        s.attach_stream(Role::Responder);
        s.on_message(Message::offer("o"));
        s.on_message(Message::answer("a"));
        s.on_message(Message::ice_candidate("c"));

        let calls = calls.lock().unwrap();
        assert_eq!(calls.accepted_offer, ["o"]);
        assert_eq!(calls.accepted_answer, ["a"]);
        assert_eq!(calls.remote_candidates, ["c"]);
    }

    /// 字段缺失只忽略，不终止会话——对端实现可能更宽松。
    #[test]
    fn malformed_messages_are_ignored_not_fatal() {
        let (mut s, calls) = session_with([]);
        s.attach_stream(Role::Responder);
        s.on_message(Message {
            ty: Some(MessageType::SdpOffer),
            data: None,
        });
        s.on_message(Message {
            ty: None,
            data: Some("x".into()),
        });

        assert!(!s.is_finished());
        assert!(calls.lock().unwrap().accepted_offer.is_empty());
    }

    #[test]
    fn fail_is_idempotent() {
        let (mut s, _) = session_with([]);
        s.fail(Error::SignalingAborted("first".into()));
        s.fail(Error::SignalingAborted("second".into()));

        assert!(matches!(poll(&mut s), Poll::Ready(Action::Failed(_))));
        assert!(matches!(poll(&mut s), Poll::Pending), "只上报一次");
    }

    /// 建连前对端关流是中止；建连后关流是 spec 步骤 8 的正常收尾。
    #[test]
    fn stream_close_before_connect_is_abort() {
        let (mut s, _) = session_with([]);
        s.attach_stream(Role::Responder);
        s.on_stream_closed();
        assert!(matches!(poll(&mut s), Poll::Ready(Action::Failed(_))));
    }

    #[test]
    fn stream_close_after_connect_is_noop() {
        let (mut s, _) = session_with([BackendEvent::Connected]);
        s.attach_stream(Role::Responder);
        assert!(matches!(poll(&mut s), Poll::Ready(Action::Connected)));

        s.on_stream_closed();
        assert!(matches!(poll(&mut s), Poll::Pending), "不应报失败");
    }
}
