//! WebRTC backend abstraction.
//!
//! The signaling state machine ([`crate::swarm::handler`]) only cares about *when* to send
//! *which* message, not about who produces the SDP. This trait separates the two, so the
//! state machine can be fully tested without a real WebRTC stack and each target can
//! specialize:
//!
//! | Target | Implementation |
//! |---|---|
//! | native | `webrtc-rs` 0.20 的 `PeerConnection` |
//! | wasm | the browser's `RTCPeerConnection` (web-sys) |
//!
//! # Why poll rather than an async trait
//!
//! `ConnectionHandler` is itself poll-driven, so making the backend an async trait would
//! only push a pile of `BoxFuture`s into the handler. The poll style matches the host and
//! sidesteps the `Send` trouble async traits run into on wasm.
//!
//! # On `Send`
//!
//! `ConnectionHandler: Send` is a hard libp2p constraint, while the browser's
//! `RtcPeerConnection` is not `Send`. The official `webrtc-websys` wraps it in
//! `SendWrapper` (wasm is single-threaded and access from another thread panics, which
//! makes it sound) — the wasm backend does the same, so there is no need to relax the
//! constraint here.

use std::task::{Context, Poll};

use libp2p_core::muxing::StreamMuxerBox;

use crate::protocol::message::MessageType;

/// Backend error.
///
/// Carried as a string: the underlying error types of the two targets have nothing in
/// common, and forcing a shared enum would only produce a pile of variants that occur on
/// one side only. The diagnostic detail is preserved in the message.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct BackendError(pub String);

impl BackendError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// Events produced by a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    /// The local SDP is ready and should be sent to the remote over the signaling stream.
    LocalDescription { ty: MessageType, sdp: String },
    /// A local ICE candidate is ready (trickle ICE, spec step 7).
    ///
    /// Candidates are produced **incrementally** and must not be batched until complete —
    /// sending as you go is the entire point of trickle.
    LocalCandidate(String),
    /// The direct connection is established (spec step 8).
    Connected,
    /// Establishing the connection failed.
    Failed(String),
}

/// A WebRTC connection backend.
///
/// Method names follow the spec's connection-establishment steps, so the code can be read
/// side by side with `webrtc.md`.
pub trait Backend: Send + 'static {
    /// As the initiator: create the `init` DataChannel and the SDP offer (spec step 4).
    ///
    /// The spec requires this DataChannel labelled `init` to be created first; without it
    /// the SDP carries no ICE information.
    fn start_offer(&mut self) -> Result<(), BackendError>;

    /// As the responder: accept the remote's offer and produce an answer (spec step 5).
    fn accept_offer(&mut self, sdp: &str) -> Result<(), BackendError>;

    /// As the initiator: accept the remote's answer (spec step 6).
    fn accept_answer(&mut self, sdp: &str) -> Result<(), BackendError>;

    /// A remote ICE candidate arrived (spec step 7).
    fn add_remote_candidate(&mut self, json: &str) -> Result<(), BackendError>;

    /// Takes the next backend event.
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<BackendEvent>;

    /// Takes the data plane.
    ///
    /// Valid only after [`BackendEvent::Connected`], and **only once** — ownership moves to
    /// the `Connection` above. Returns `None` if not yet connected or already taken.
    fn take_muxer(&mut self) -> Option<StreamMuxerBox>;
}

/// Backend factory: one fresh backend per signaling stream.
pub type Factory = std::sync::Arc<
    dyn Fn(&crate::Config) -> Result<Box<dyn Backend>, BackendError> + Send + Sync + 'static,
>;

/// Native backend (`webrtc-rs`). Not compiled on wasm — it ships its own UDP/SCTP/DTLS
/// stack.
#[cfg(not(target_family = "wasm"))]
pub mod native;

/// Browser backend (the native RTCPeerConnection). Compiled on wasm only.
#[cfg(target_family = "wasm")]
pub mod wasm;

/// Test double: a scripted backend that drives the signaling state machine without a real
/// WebRTC stack.
#[cfg(test)]
pub(crate) mod mock;
