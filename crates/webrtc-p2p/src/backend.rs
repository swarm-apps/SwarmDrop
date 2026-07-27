//! WebRTC 后端抽象。
//!
//! 信令状态机（[`crate::handler`]）只关心「什么时候该发什么消息」，不关心 SDP 由谁生成。
//! 这层 trait 把两者切开，于是状态机可以脱离真实 WebRTC 栈被完整测试，两个 target 也能
//! 各自特化：
//!
//! | target | 实现 |
//! |---|---|
//! | native | `webrtc-rs` 0.20 的 `PeerConnection` |
//! | wasm | 浏览器 `RTCPeerConnection`（web-sys） |
//!
//! # 为什么是 poll 而不是 async trait
//!
//! `ConnectionHandler` 本身是 poll 驱动的，后端做成 async trait 反而要在 handler 里存一堆
//! `BoxFuture`。poll 风格与宿主一致，也回避了 async trait 在 wasm 上的 `Send` 麻烦。
//!
//! # 关于 `Send`
//!
//! `ConnectionHandler: Send` 是 libp2p 的硬约束，而浏览器的 `RtcPeerConnection` 不是
//! `Send`。官方 `webrtc-websys` 的做法是用 `SendWrapper` 包住（wasm 单线程，非创建线程
//! 访问会 panic，因而安全）——wasm 后端照此办理即可，不必为它松开这里的约束。

use std::task::{Context, Poll};

use crate::signaling::MessageType;

/// 后端错误。
///
/// 用字符串承载：两个 target 的底层错误类型毫无共同点，硬造一个共同枚举只会得到一堆
/// 只在单边出现的变体。诊断信息保留在文案里。
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct BackendError(pub String);

impl BackendError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// 后端产出的事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    /// 本地 SDP 就绪，应经信令流发给对端。
    LocalDescription { ty: MessageType, sdp: String },
    /// 本地 ICE 候选就绪（trickle ICE，spec 步骤 7）。
    ///
    /// 候选是**陆续**产生的，不能等齐了再发——trickle 的全部意义就是边发边连。
    LocalCandidate(String),
    /// 直连已建立（spec 步骤 8）。
    Connected,
    /// 建连失败。
    Failed(String),
}

/// WebRTC 连接后端。
///
/// 方法命名对应 spec 的连接建立步骤，便于与 `webrtc.md` 对照阅读。
pub trait Backend: Send + 'static {
    /// 作为发起方：创建 `init` DataChannel 与 SDP offer（spec 步骤 4）。
    ///
    /// spec 要求必须先建这条 label 为 `init` 的 DataChannel，否则 SDP 里不带 ICE 信息。
    fn start_offer(&mut self) -> Result<(), BackendError>;

    /// 作为应答方：接受对端 offer 并生成 answer（spec 步骤 5）。
    fn accept_offer(&mut self, sdp: &str) -> Result<(), BackendError>;

    /// 作为发起方：接受对端 answer（spec 步骤 6）。
    fn accept_answer(&mut self, sdp: &str) -> Result<(), BackendError>;

    /// 收到对端的 ICE 候选（spec 步骤 7）。
    fn add_remote_candidate(&mut self, json: &str) -> Result<(), BackendError>;

    /// 取下一个后端事件。
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<BackendEvent>;
}

/// 后端工厂：每条信令流配一个新后端。
pub type Factory = std::sync::Arc<
    dyn Fn(&crate::Config) -> Result<Box<dyn Backend>, BackendError> + Send + Sync + 'static,
>;

#[cfg(test)]
pub(crate) mod mock {
    //! 脚本化后端，用于把信令状态机与真实 WebRTC 栈解耦测试。

    use super::*;
    use std::collections::VecDeque;

    /// 记录状态机对后端的调用，供断言「该调的调了、不该调的没调」。
    #[derive(Debug, Default, PartialEq, Eq)]
    pub(crate) struct Calls {
        pub started_offer: bool,
        pub accepted_offer: Vec<String>,
        pub accepted_answer: Vec<String>,
        pub remote_candidates: Vec<String>,
    }

    pub(crate) struct MockBackend {
        /// 按序吐出的事件；空了就 Pending。
        pub queued: VecDeque<BackendEvent>,
        pub calls: std::sync::Arc<std::sync::Mutex<Calls>>,
        /// 令某个调用失败，用于测错误路径。
        pub fail_on_offer: bool,
    }

    impl MockBackend {
        pub(crate) fn new(
            events: impl IntoIterator<Item = BackendEvent>,
        ) -> (Self, std::sync::Arc<std::sync::Mutex<Calls>>) {
            let calls = std::sync::Arc::new(std::sync::Mutex::new(Calls::default()));
            (
                Self {
                    queued: events.into_iter().collect(),
                    calls: calls.clone(),
                    fail_on_offer: false,
                },
                calls,
            )
        }
    }

    impl Backend for MockBackend {
        fn start_offer(&mut self) -> Result<(), BackendError> {
            if self.fail_on_offer {
                return Err(BackendError::new("mock: offer 失败"));
            }
            self.calls.lock().unwrap().started_offer = true;
            Ok(())
        }

        fn accept_offer(&mut self, sdp: &str) -> Result<(), BackendError> {
            self.calls.lock().unwrap().accepted_offer.push(sdp.into());
            Ok(())
        }

        fn accept_answer(&mut self, sdp: &str) -> Result<(), BackendError> {
            self.calls.lock().unwrap().accepted_answer.push(sdp.into());
            Ok(())
        }

        fn add_remote_candidate(&mut self, json: &str) -> Result<(), BackendError> {
            self.calls
                .lock()
                .unwrap()
                .remote_candidates
                .push(json.into());
            Ok(())
        }

        fn poll(&mut self, _cx: &mut Context<'_>) -> Poll<BackendEvent> {
            match self.queued.pop_front() {
                Some(e) => Poll::Ready(e),
                None => Poll::Pending,
            }
        }
    }
}
