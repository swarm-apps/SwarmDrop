//! 内核错误类型。
//!
//! 按操作分类（connect / open / rpc / accept），不做统一大枚举——
//! 调用方在每个 API 上只面对可能发生的失败。

use swarmdrop_net_base::{NodeId, ProtocolId};

/// 通用失败（状态查询、命令下发等）。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Endpoint 已关闭（actor 已退出，所有命令失效）。
    #[error("endpoint is closed")]
    Closed,
}

/// `Endpoint::connect` 的失败。
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("endpoint is closed")]
    Closed,
    /// 既无已知地址、AddressLookup 也解析不出（或未配置）。
    #[error("no known addresses for {0}")]
    NoAddresses(NodeId),
    /// 拨号失败（所有候选地址都失败）。
    #[error("dial failed: {0}")]
    DialFailed(String),
    #[error("connect timed out")]
    Timeout,
}

/// `Endpoint::open` 的失败。
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("endpoint is closed")]
    Closed,
    #[error(transparent)]
    Connect(#[from] ConnectError),
    /// 对端不支持该协议（multistream-select 协商失败）。
    #[error("remote does not support {0}")]
    UnsupportedProtocol(ProtocolId),
    /// 超出 per-peer / per-protocol 活跃流配额。
    #[error("stream limit exceeded for {0}")]
    LimitExceeded(ProtocolId),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// RPC（一流一问一答）调用方失败。
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error("encode request: {0}")]
    Encode(String),
    #[error("decode response: {0}")]
    Decode(String),
    /// 对端在发回响应前关闭了流。
    #[error("remote closed the stream before responding")]
    ClosedEarly,
    #[error("frame too large: {0} bytes")]
    FrameTooLarge(usize),
    #[error("rpc timed out")]
    Timeout,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// [`ProtocolHandler::accept`](crate::ProtocolHandler::accept) 的失败。
///
/// Router 对任何 `Err` 的处理都是记一行 warn 然后 drop 流——不会给对端发
/// 专门的错误码。要让对端知道原因，handler 内自行写响应帧后再返回。
#[derive(Debug, thiserror::Error)]
pub enum AcceptError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode request: {0}")]
    Decode(String),
    /// 业务层错误（经 [`AcceptError::from_err`] 包装）。
    #[error("{0}")]
    User(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl AcceptError {
    /// 把任意业务错误包进 `User` 变体。
    pub fn from_err(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::User(Box::new(err))
    }
}
