//! Protocol layer: the wire format of `/webrtc-signaling/0.0.1` and the address
//! conventions.
//!
//! **This layer does not depend on `libp2p-swarm`**; it uses only `Multiaddr` from
//! `libp2p-core` and `PeerId` from `libp2p-identity`. Two benefits follow:
//!
//! - reviewing it against the spec does not require working around libp2p's poll model
//! - changing hosts (a different libp2p version, or none at all) leaves this layer usable
//!   as-is
//!
//! Dependencies point one way: `swarm` → `backend` → `protocol`. This layer never
//! references any layer above it.

pub mod addr;
pub mod codec;
pub mod message;

pub use codec::Codec;
pub use message::{MAX_MESSAGE_LEN, Message, MessageType, SIGNALING_PROTOCOL};
