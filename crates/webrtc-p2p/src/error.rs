//! 顶层错误。
//!
//! 分层聚合而非大杂烩：各层保有自己的错误类型（[`protocol::addr::Error`]、
//! [`protocol::message::Error`]、[`backend::BackendError`]），这里只做汇总，外加几种
//! 只属于「会话流程」的失败。
//!
//! [`protocol::addr::Error`]: crate::protocol::addr::Error
//! [`protocol::message::Error`]: crate::protocol::message::Error
//! [`backend::BackendError`]: crate::backend::BackendError

use libp2p_identity::PeerId;

/// 本传输的错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 地址不是本传输能处理的形态。
    #[error("地址不可用：{0}")]
    Addr(#[from] crate::protocol::addr::Error),

    /// 信令消息的编解码或流读写失败。
    #[error("信令协议错误：{0}")]
    Protocol(#[from] crate::protocol::message::Error),

    /// Transport 与 Behaviour 失联。
    ///
    /// 唯一成因是两者未注册进同一个 Swarm，或 Behaviour 已被 drop——属于**装配错误**，
    /// 不是运行时故障，故文案直指装配而非网络。
    #[error(
        "behaviour 未注册或已释放：Transport 与 Behaviour 必须由 `new()` 配对产出并注册进同一个 Swarm"
    )]
    BehaviourDetached,

    #[error("与 {peer} 的信令超时")]
    SignalingTimeout { peer: PeerId },

    /// 信令流被中止：对端 reset、连接断开、开流失败等。
    #[error("信令中止：{0}")]
    SignalingAborted(String),

    /// WebRTC 建连本身失败（后端上报）。
    #[error("WebRTC 连接失败：{0}")]
    Connection(String),
}
