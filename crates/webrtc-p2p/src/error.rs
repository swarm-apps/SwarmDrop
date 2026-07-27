//! 传输错误。

use libp2p_identity::PeerId;

/// 本传输的错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("地址不可用：{0}")]
    Addr(#[from] crate::addr::Error),

    #[error("信令消息编解码失败：{0}")]
    Signaling(#[from] crate::signaling::Error),

    /// Transport 与 Behaviour 失联。
    ///
    /// 唯一成因是两者未注册进同一个 Swarm，或 Behaviour 已被 drop——属于装配错误，
    /// 不是运行时故障，故错误文案直指装配。
    #[error(
        "behaviour 未注册或已释放：Transport 与 Behaviour 必须由 `new()` 配对产出并注册进同一个 Swarm"
    )]
    BehaviourDetached,

    #[error("与 {peer} 的信令超时")]
    SignalingTimeout { peer: PeerId },

    #[error("对端拒绝或重置了信令流：{0}")]
    SignalingAborted(String),

    #[error("WebRTC 连接建立失败：{0}")]
    Connection(String),
}
