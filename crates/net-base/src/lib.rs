//! SwarmDrop 网络类型底座。
//!
//! 跨 FFI / IPC / entity 复用的身份与地址类型，libp2p 类型在此收口成
//! newtype 后不再向上穿透（`why-libp2p-not-iroh.md` 的关键边界）。
//!
//! - [`NodeId`] / [`SecretKey`] —— 节点身份（ed25519）。`NodeId` 的 base58
//!   字符串与旧栈 `PeerId` 完全一致，数据库/keychain 存量无需迁移。
//! - [`Addr`] / [`NodeAddr`] —— 网络地址（Multiaddr newtype + 可拨性分类谓词）。
//! - [`ProtocolId`] —— 流协议标识（`/swarmdrop/pairing/2` 形态）。
//! - [`NatStatus`] / [`PathKind`] / [`DiscoverySource`] —— 跨 IPC 的状态枚举。

mod addr;
mod node_id;
mod protocol_id;
mod status;

pub use addr::{Addr, AddrParseError, NodeAddr};
pub use node_id::{IdParseError, NodeId, SecretKey};
pub use protocol_id::{InvalidProtocolId, ProtocolId};
pub use status::{DiscoverySource, NatStatus, PathKind};
