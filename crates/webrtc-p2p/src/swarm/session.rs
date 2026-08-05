//! Signaling session state machine.
//!
//! **Pure logic: no I/O, no dependency on `libp2p-swarm`.** It consumes commands, remote
//! messages and backend events, and produces "what the host should do" ([`Action`]). The
//! actual reads and writes are performed by [`super::handler`].
//!
//! The split exists because the state machine is the most bug-prone part of this crate —
//! lifted out, it can be driven step by step synchronously and asserted on one transition
//! at a time, with no real `Stream` to construct and no poll model to enter.
//!
//! # Role symmetry
//!
//! Spec step 4 is a MUST: each side must be able to both offer and answer. "A initiates"
//! is only a convention to stop both sides from starting at once and building two
//! connections; it is not a division of capability.
//!
//! | Role | Triggered by | First action |
//! |---|---|---|
//! | [`Role::Initiator`] | a command from the behaviour | [`Action::OpenStream`] → `start_offer` |
//! | [`Role::Responder`] | an inbound stream from the remote | await the offer → `accept_offer` |

use std::collections::VecDeque;
use std::task::{Context, Poll};

use futures::FutureExt;
use futures_timer::Delay;

use crate::backend::{Backend, BackendEvent, Factory};
use crate::config::Config;
use crate::error::Error;
use crate::protocol::{Message, MessageType};

/// This side's role in the current round of signaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Initiating: opens an outbound stream and sends the offer.
    Initiator,
    /// Responding: accepts an inbound stream and replies with the answer.
    Responder,
}

/// Actions the session asks the host to perform.
#[derive(Debug)]
pub enum Action {
    /// An outbound signaling stream is needed (initiator only).
    OpenStream,
    /// The direct connection is established (spec step 8).
    Connected,
    /// The session failed.
    Failed(Error),
}

/// Session lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Role not yet determined; not started.
    Idle,
    /// Signaling in progress.
    Signaling,
    /// Terminated (successfully or not); produces no further actions.
    Finished,
}

/// A signaling session.
pub struct Session {
    config: Config,
    factory: Factory,
    state: State,
    role: Option<Role>,
    backend: Option<Box<dyn Backend>>,
    /// Messages queued for the remote. The host drains them with
    /// [`Session::next_outgoing`] once the stream is writable.
    ///
    /// Owned by the session rather than the host: candidates arrive **incrementally**, and
    /// while the stream is not ready they must be buffered rather than dropped — one lost
    /// trickle-ICE candidate can mean one fewer usable path.
    outbox: VecDeque<Message>,
    /// Whether the host has already been asked to open an outbound stream, so the request
    /// is not repeated on every poll.
    stream_requested: bool,
    stream_attached: bool,
    /// Signaling timeout timer. Started when the session actually begins (once the role is
    /// determined) and dropped on termination.
    ///
    /// Without it, an unresponsive remote would pin the session — and the relay connection
    /// it lives on — forever: the handler's keep-alive rule is "hold while the session is
    /// unfinished", and the session would never finish.
    timeout: Option<Delay>,
    /// The terminal action (Connected / Failed) waiting to be picked up by the host.
    ///
    /// `fail()` may be called outside of a poll (opening the stream failed, a stream read or
    /// write errored), so the action is stashed and handed over on the next poll.
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
            timeout: None,
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
        self.begin_signaling();
    }

    /// 进入信令态并启动超时。
    fn begin_signaling(&mut self) {
        self.state = State::Signaling;
        if self.timeout.is_none() {
            self.timeout = Some(Delay::new(self.config.signaling_timeout()));
        }
    }

    /// 信令流就绪。
    ///
    /// 入站流会顺带把角色定为应答方——**这一步不需要 behaviour 参与**，正是「能被拨」
    /// 的实现所在。
    pub fn attach_stream(&mut self, role: Role) {
        if self.state == State::Finished || self.stream_attached {
            tracing::debug!(?role, state = ?self.state, attached = self.stream_attached,
                "忽略 attach_stream");
            return;
        }
        self.role.get_or_insert(role);
        self.begin_signaling();
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
        tracing::debug!(%error, role = ?self.role, "信令会话失败");
        self.state = State::Finished;
        self.backend = None;
        self.outbox.clear();
        self.timeout = None;
        self.pending_action = Some(Action::Failed(error));
    }

    /// 取出数据面。仅在 [`Action::Connected`] 之后有效，且只能取一次。
    pub fn take_muxer(&mut self) -> Option<libp2p_core::muxing::StreamMuxerBox> {
        self.backend.as_mut()?.take_muxer()
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

        if let Some(timer) = self.timeout.as_mut()
            && timer.poll_unpin(cx).is_ready()
        {
            self.fail(Error::SignalingTimeout);
            return Poll::Ready(self.pending_action.take().expect("fail 必然置入动作"));
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
                    // 注意此处**不清 backend**：数据面还要靠它交出来（见 take_muxer）。
                    self.state = State::Finished;
                    self.timeout = None;
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
        session_with_config(Config::default(), events)
    }

    fn session_with_config(
        config: Config,
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
        (Session::new(config, factory), calls)
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

    // 超时**真正触发**的行为在集成测试里验（`tests/native_signaling.rs`）——
    // 同步单测用的是 noop waker，计时器到点也叫不醒它。这里只验启停时机。

    /// 建连成功后计时器必须停——否则一条正常连接会在超时点被判失败。
    #[test]
    fn success_stops_the_clock() {
        let (mut s, _) = session_with([BackendEvent::Connected]);
        s.attach_stream(Role::Responder);
        assert!(s.timeout.is_some(), "信令期间应在计时");

        assert!(matches!(poll(&mut s), Poll::Ready(Action::Connected)));
        assert!(s.timeout.is_none(), "成功后计时器应被丢弃");
    }

    /// 计时器在角色确定时才启动：尚未开始的会话不该被判超时。
    #[test]
    fn idle_session_has_no_clock() {
        let (mut s, _) = session_with([]);
        assert!(s.timeout.is_none(), "未定角色前不应计时");
        s.attach_stream(Role::Responder);
        assert!(s.timeout.is_some(), "会话开始后应计时");
    }

    /// 失败也要停表，否则 fail 之后计时器还挂着。
    #[test]
    fn failure_stops_the_clock() {
        let (mut s, _) = session_with([]);
        s.attach_stream(Role::Responder);
        s.fail(Error::SignalingAborted("x".into()));
        assert!(s.timeout.is_none());
    }
}
