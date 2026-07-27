//! 建立完成的 WebRTC 连接。
//!
//! # 现状
//!
//! **后端尚未接入。** 本类型现在只是两个平面之间传递的占位句柄，用来把 transport /
//! behaviour 的装配先立起来并保持类型收敛。
//!
//! 接后端时它会持有一条 `RTCPeerConnection` 并实现 `StreamMuxer`：
//!
//! | target | 底层 |
//! |---|---|
//! | native | `webrtc-rs` 0.20 的 `PeerConnection` |
//! | wasm | 浏览器 `RTCPeerConnection`（web-sys） |
//!
//! 两者之上的 DataChannel → libp2p `Stream` 适配复用 `libp2p-webrtc-utils` 的
//! `Stream<T>`（泛型、不依赖 webrtc-rs，故不会把 0.17 拖进来）。

use libp2p_identity::PeerId;

/// 一条建立完成的 WebRTC 连接。
///
/// TODO(后端)：持有 PeerConnection 并实现 `libp2p_core::muxing::StreamMuxer`。
/// 届时 `Transport::Output` 从 `(PeerId, Connection)` 经 `.map()` 转成
/// `(PeerId, StreamMuxerBox)`，与 libp2p-webrtc 的用法一致。
#[derive(Debug)]
pub struct Connection {
    peer: PeerId,
}

impl Connection {
    #[allow(dead_code, reason = "待 handler 落地后由信令完成时构造")]
    pub(crate) fn new(peer: PeerId) -> Self {
        Self { peer }
    }

    /// 对端身份。
    ///
    /// 由信令所经的**已认证** relay 连接确定，而非对端自报——SDP 里的 DTLS 指纹在
    /// 握手时被验证，两者共同完成身份绑定（见 crate 文档「为什么不需要额外的 Noise」）。
    pub fn peer(&self) -> PeerId {
        self.peer
    }
}
