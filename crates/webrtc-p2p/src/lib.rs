//! libp2p WebRTC transport, in two modes.
//!
//! | Mode | Address | When it applies |
//! |---|---|---|
//! | **Hole punching** | `<relayed-multiaddr>/webrtc/p2p/…` | Neither side can listen (browser / behind NAT) |
//! | **Direct** | `/ip4/…/udp/…/webrtc-direct/certhash/…` | One side has a reachable address (public / same subnet) |
//!
//! Hole punching implements `/webrtc-signaling/0.0.1` from the libp2p spec
//! [`webrtc/webrtc.md`] (the spec calls it private-to-private; it corresponds to
//! js-libp2p's `webRTC()`). Direct implements [`webrtc/webrtc-direct.md`] and is a
//! **complete replacement** for the official `libp2p-webrtc` / `libp2p-webrtc-websys`
//! — both build on webrtc-rs but on different versions (0.20 vs 0.17), so keeping them
//! side by side means compiling the whole ICE/DTLS/SCTP stack twice.
//!
//! [`webrtc/webrtc.md`]: https://github.com/libp2p/specs/blob/master/webrtc/webrtc.md
//! [`webrtc/webrtc-direct.md`]: https://github.com/libp2p/specs/blob/master/webrtc/webrtc-direct.md
//!
//! # What separates the two modes: which channel carries the certificate fingerprint
//!
//! | | Hole punching | Direct |
//! |---|---|---|
//! | Fingerprint channel | the **authenticated** relay connection | the multiaddr — **untrusted** |
//! | Peer authentication | binding the DTLS fingerprint is enough | **a second Noise handshake is required** |
//! | ICE | full, both sides gather candidates | server is ICE-lite, answers passively only |
//! | Signaling | `/webrtc-signaling/0.0.1` | none — the SDP is derived deterministically from the multiaddr |
//!
//! Hole punching needs no extra handshake: the SDP carries the DTLS certificate
//! fingerprint, the SDP travels over an authenticated relay connection, and the DTLS
//! handshake verifies it — which binds the identity (spec FAQ, first entry).
//!
//! ⚠️ This turns "the relay connection must be authenticated" into a **security
//! precondition** rather than an implementation detail. libp2p relay connections are
//! themselves authenticated via Noise/TLS, so it holds today; but the moment signaling
//! is allowed over an unauthenticated channel, the whole model collapses.
//!
//! A direct-mode certhash, by contrast, may reach the dialer over any channel at all
//! (pasted on a web page, printed in a QR code), so the Noise handshake on that path
//! **cannot be skipped** — DTLS proves "the other side holds this certificate", not
//! "this certificate belongs to that PeerId".
//!
//! # Symmetry is a spec MUST, not an optional optimization (hole-punching mode)
//!
//! Spec step 4, verbatim: *"A MUST as well be able to handle an incoming signaling
//! protocol stream to support the case where B initiates the signaling process."*
//!
//! That is, **each side must be able to both offer and answer**. "A initiates" is only a
//! convention to stop both sides from starting at once and building two connections; it
//! is not a division of capability. Both target specializations in this crate must
//! implement the full bidirectional behaviour — upstream PR #5978 only did the browser
//! side, which covers web↔web but never reaches web↔peer-behind-NAT.
//!
//! # Layering
//!
//! Dependencies point one way: `swarm` → `backend` → `protocol`. Lower layers never
//! reference upper ones.
//!
//! | Layer | Responsibility | Depends on libp2p-swarm |
//! |---|---|---|
//! | [`protocol`] | Wire format: message encoding, framed codec, address conventions for both modes | No |
//! | [`backend`] | WebRTC stack abstraction; native / wasm each specialize it | No |
//! | [`swarm`] | Wiring into the `Transport` and `NetworkBehaviour` planes | Yes |
//!
//! [`backend`] is the only one of the three bound to a concrete WebRTC implementation;
//! the state machine ([`swarm::session`]) and the protocol layer are pure logic and can
//! be tested without a real WebRTC stack or a real `Stream`.
//!
//! Direct mode has no signaling and therefore involves neither `session` nor
//! `Behaviour` — it is self-contained inside [`backend`], and [`Transport`] dispatches
//! to it based on the address segments.

pub mod backend;
mod config;
pub mod error;
pub mod protocol;
pub mod swarm;

/// DTLS certificate for direct mode (native).
///
/// The host uses it to generate and persist a certificate: store
/// `Certificate::generate()?.serialize_pem()` on disk and load it back on the next
/// start via `DirectConfig::with_certificate_pem(pem)`. **Persisting it is mandatory** —
/// otherwise the certhash in the advertised address changes on every restart.
#[cfg(not(target_family = "wasm"))]
pub use backend::native::direct::Certificate;
pub use backend::{Backend, BackendError, BackendEvent, Factory};
pub use config::{Config, DirectConfig};
pub use error::Error;
pub use protocol::{Message, MessageType, SIGNALING_PROTOCOL};
pub use swarm::{Behaviour, Connection, Event, Transport};

/// Creates a paired [`Transport`] and [`Behaviour`].
///
/// The two **must be registered with the same Swarm**: establishing a connection through
/// the transport requires the behaviour to open a signaling stream over the relay
/// connection (the reasoning lives in the [`swarm`] module). If only one of them is
/// registered, dialing fails fast with [`Error::BehaviourDetached`] instead of hanging
/// silently.
///
/// `factory` creates one [`Backend`] per signaling stream. It is injected by the caller
/// rather than built in because the two targets share nothing in their WebRTC stacks
/// (native = `webrtc-rs`, wasm = the browser's `RTCPeerConnection`), and because it lets
/// the state machine be tested without a real WebRTC stack.
///
/// ```no_run
/// use std::sync::Arc;
/// # use webrtc_p2p::{Backend, BackendError, Config};
/// # fn make_backend() -> Result<Box<dyn Backend>, BackendError> { unimplemented!() }
/// let (transport, behaviour) = webrtc_p2p::new(
///     Config::default(),
///     Arc::new(|_cfg: &Config| make_backend()),
/// );
/// // Wire the transport in via `with_other_transport`; put the behaviour into your
/// // derived NetworkBehaviour struct.
/// # let _ = (transport, behaviour);
/// ```
pub fn new(config: Config, factory: Factory) -> (Transport, Behaviour) {
    let (transport_side, behaviour_side) = swarm::channel::pair();
    (
        Transport::new(config.clone(), transport_side),
        Behaviour::new(config, factory, behaviour_side),
    )
}
