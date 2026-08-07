//! Top-level error type.
//!
//! An aggregation of layers rather than a catch-all: each layer keeps its own error type
//! ([`protocol::addr::Error`], [`protocol::message::Error`], [`backend::BackendError`]),
//! and this one merely gathers them, plus a few failures that belong solely to the
//! "session flow".
//!
//! [`protocol::addr::Error`]: crate::protocol::addr::Error
//! [`protocol::message::Error`]: crate::protocol::message::Error
//! [`backend::BackendError`]: crate::backend::BackendError

/// Errors produced by this transport.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The address is not in a shape this transport can handle.
    #[error("address unavailable: {0}")]
    Addr(#[from] crate::protocol::addr::Error),

    /// Encoding/decoding a signaling message, or reading/writing the stream, failed.
    #[error("signaling protocol error: {0}")]
    Protocol(#[from] crate::protocol::message::Error),

    /// The Transport lost contact with the Behaviour.
    ///
    /// The only cause is that the two were not registered with the same Swarm, or that the
    /// Behaviour has been dropped — this is a **wiring mistake**, not a runtime fault, so
    /// the message points at the wiring rather than at the network.
    #[error(
        "behaviour 未注册或已释放：Transport 与 Behaviour 必须由 `new()` 配对产出并注册进同一个 Swarm"
    )]
    BehaviourDetached,

    /// Signaling did not complete within
    /// [`Config::signaling_timeout`](crate::Config::signaling_timeout).
    ///
    /// Carries no peer identity: a session is per-connection and does not know who the
    /// remote is; the behaviour fills that in when reporting.
    #[error("signaling timed out")]
    SignalingTimeout,

    /// The signaling stream was aborted: the remote reset it, the connection dropped,
    /// opening the stream failed, and so on.
    #[error("signaling aborted: {0}")]
    SignalingAborted(String),

    /// Establishing the WebRTC connection itself failed (reported by the backend).
    #[error("WebRTC connection failed: {0}")]
    Connection(String),
}
