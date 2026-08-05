//! libp2p integration layer: wiring the signaling session into the `Transport` and
//! `NetworkBehaviour` planes.
//!
//! # Why this is split into four pieces
//!
//! libp2p keeps "dialing" and "protocol streams" on two planes that cannot reach each
//! other, while establishing a connection through this transport requires **an already
//! established connection** before SDP can be exchanged. That forces the responsibilities
//! apart; the boundaries are:
//!
//! | Module | Responsibility | Depends on libp2p |
//! |---|---|---|
//! | [`session`] | Signaling session state machine — what to do on each input | **No** (pure logic, independently testable) |
//! | [`handler`] | Adapts the session onto the `ConnectionHandler` poll model | Yes |
//! | [`behaviour`] | Connection management, dial bookkeeping, round trips with the transport | Yes |
//! | [`transport`] | `Transport` trait implementation, address dispatch | Yes |
//!
//! **Decoupling [`session`] from libp2p is deliberate**: the state machine is the part
//! most prone to bugs, and lifting it out of the poll adapter lets it be driven step by
//! step synchronously and asserted on one transition at a time, with no need to construct
//! a real `Stream`.

pub mod behaviour;
pub(crate) mod channel;
pub mod connection;
pub(crate) mod direct;
pub mod handler;
pub mod session;
pub mod transport;

pub use behaviour::{Behaviour, Event};
pub use connection::Connection;
pub use transport::Transport;
