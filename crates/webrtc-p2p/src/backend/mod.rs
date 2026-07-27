//! WebRTC 后端抽象。
//!
//! 信令状态机（[`crate::swarm::handler`]）只关心「什么时候该发什么消息」，不关心 SDP 由谁生成。
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

use libp2p_core::muxing::StreamMuxerBox;

use crate::protocol::message::MessageType;

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

    /// 取出数据面。
    ///
    /// 仅在 [`BackendEvent::Connected`] 之后有效，且**只能取一次**——所有权交给上层的
    /// `Connection`。未建连或已取走时返回 `None`。
    fn take_muxer(&mut self) -> Option<StreamMuxerBox>;
}

/// 后端工厂：每条信令流配一个新后端。
pub type Factory = std::sync::Arc<
    dyn Fn(&crate::Config) -> Result<Box<dyn Backend>, BackendError> + Send + Sync + 'static,
>;

/// 原生后端（`webrtc-rs`）。wasm 上不编——它自带 UDP/SCTP/DTLS 栈。
#[cfg(not(target_family = "wasm"))]
pub mod native;

/// 浏览器后端（原生 RTCPeerConnection）。仅 wasm 上编。
#[cfg(target_family = "wasm")]
pub mod wasm;

/// 测试替身：脚本化后端，让信令状态机脱离真实 WebRTC 栈被驱动。
#[cfg(test)]
pub(crate) mod mock;
