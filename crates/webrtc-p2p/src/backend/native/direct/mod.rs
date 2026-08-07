//! Native-side implementation of direct mode (`/webrtc-direct`).
//!
//! Corresponds to the libp2p spec [`webrtc/webrtc-direct.md`]: one end has a reachable
//! address (public or on the same subnet) and the other (usually a browser) dials it
//! directly, with **no signaling** — the SDP is derived deterministically from the
//! multiaddr.
//!
//! [`webrtc/webrtc-direct.md`]: https://github.com/libp2p/specs/blob/master/webrtc/webrtc-direct.md
//!
//! # Division of labour with hole-punching mode
//!
//! These are two **non-overlapping** connection-establishment paths in one crate; do not
//! merge them in the name of writing less code:
//!
//! | | Hole punching (the rest of [`super`]) | Direct (this module) |
//! |---|---|---|
//! | ICE | Full agent, both sides gather candidates | **ICE-lite**: the server only answers passively |
//! | Signaling | `/webrtc-signaling/0.0.1` over the relay | None |
//! | Fingerprint channel | the authenticated relay connection | the multiaddr, **untrusted** |
//! | Peer authentication | binding the DTLS fingerprint suffices | **a second Noise handshake is required** |
//!
//! The last row is the first entry of the spec FAQ, and the easiest thing to get wrong by
//! cutting corners: a direct-mode certhash may travel over any untrusted channel (pasted
//! on a web page, embedded in a QR code), and only the Noise handshake can prove that
//! "whoever holds this certificate really is that PeerId". Hole-punching mode does not
//! need it, because its SDP travels over an authenticated connection.

mod certificate;
mod sdp;
pub(crate) mod transport;
mod udp_mux;
mod upgrade;

pub use certificate::{Certificate, Error as CertificateError};
