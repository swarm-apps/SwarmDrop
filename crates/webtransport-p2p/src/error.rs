//! 本 crate 的错误。
//!
//! 分五类，每类对应一个**调用方能据此做不同决定**的失败原因；再细分下去就只是把
//! 底层库的错误原样抬上来，那属于日志不属于类型。
//!
//! `wtransport` 的错误类型一律在这里退化成字符串 —— 它们出现在公共 API 上就等于把
//! 用户钉死在某个 wtransport 版本，而本 crate 要独立发布（见 crate 顶部的定位注释）。
//! 代价是调用方无法按 QUIC 层的判别码分派；这是刻意的：真需要分派的判据（超时、
//! 身份不符）已各自成一个变体。

use std::time::Duration;

use libp2p_identity::PeerId;

/// WebTransport transport 的错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 证书生命周期出错：生成、解析或轮换。
    #[error("证书错误：{0}")]
    Certificate(#[from] crate::certificate::Error),

    /// 绑定监听端口失败。
    #[error("绑定 {addr} 失败：{source}")]
    Bind {
        addr: std::net::SocketAddr,
        #[source]
        source: std::io::Error,
    },

    /// QUIC / WebTransport 会话建立失败（含 CONNECT 被服务端拒绝）。
    #[error("建立 WebTransport 会话失败：{0}")]
    Connect(String),

    /// Noise 认证握手失败。**certhash 校验不通过也落在这里** ——
    /// 它由 `libp2p-noise` 在握手内部完成，对外就是一次握手失败。
    #[error("Noise 认证失败：{0}")]
    Noise(#[from] libp2p_noise::Error),

    /// 从会话建立到 Noise 握手完成超时。
    #[error("握手超时（{0:?}）")]
    Timeout(Duration),

    /// 拨号地址声明的身份与握手结果不符。
    ///
    /// **装箱不是风格**：`PeerId` 是 80 字节，两个就把整个 `Error` 撑到 160 —— 而
    /// `Error` 出现在每一个 `Result` 的 Err 侧，包括成功路径（成功路径也要为最大变体
    /// 留栈空间）。这条错误极少发生，为它让所有调用付 160 字节不划算。
    #[error("对端身份不符：地址声明 {}，实得 {}", .0.expected, .0.actual)]
    PeerIdMismatch(Box<PeerIdMismatch>),
}

/// [`Error::PeerIdMismatch`] 的载荷。
#[derive(Debug)]
pub struct PeerIdMismatch {
    /// 地址末尾 `/p2p/` 段声明的身份。
    pub expected: PeerId,
    /// Noise 握手实际认出的身份。
    pub actual: PeerId,
}

impl Error {
    pub(crate) fn peer_id_mismatch(expected: PeerId, actual: PeerId) -> Self {
        Self::PeerIdMismatch(Box::new(PeerIdMismatch { expected, actual }))
    }
}

/// 给一段建连/握手流程加超时。
///
/// 出站（`dialer`）与入站（`transport::upgrade_inbound`）都要它，且盖的是同一个
/// `Config::handshake_timeout`。分成两份手写 `select` 的话，改超时语义（加日志、区分
/// 「会话建立」与「Noise 握手」）要记着改两处，而其中一处埋在 `.boxed()` 的 async block 里。
///
/// 盖的是**整条链路**而不是每一步各加一个：分段超时的总上限是各段之和，说不清也调不准。
pub(crate) async fn with_timeout<T>(
    fut: impl std::future::Future<Output = Result<T, Error>>,
    timeout: Duration,
) -> Result<T, Error> {
    use futures::future::{Either, select};

    futures::pin_mut!(fut);
    match select(fut, futures_timer::Delay::new(timeout)).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err(Error::Timeout(timeout)),
    }
}
