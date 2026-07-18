//! SwarmDrop 网络内核。
//!
//! iroh 风格 API、libp2p 底层、native + wasm32 双 target。四层架构
//! （`dev-notes/why-libp2p-not-iroh.md`）中的「Network Runtime」层：
//! 隐藏事件循环 · 连接管理 · 协议路由 · 地址选择。
//!
//! # 形态（M1 起逐步落地）
//!
//! ```ignore
//! let endpoint = Endpoint::builder()
//!     .secret_key(sk)
//!     .preset(presets::Native)
//!     .bind()
//!     .await?;
//! let router = Router::builder(endpoint.clone())
//!     .accept(PAIRING_V2, pairing_handler)
//!     .spawn();
//! let stream = endpoint.open(node_id, TRANSFER_DATA_V2).await?;
//! ```
//!
//! 设计基线：
//! - `Endpoint` 是 `Arc<Inner>` 门面（Clone 廉价），后台单中枢 actor 是唯一的
//!   Swarm poll 点，用户永不接触事件循环；
//! - 协议按 [`base::ProtocolId`] 路由（stream 级，尊重 libp2p multistream-select
//!   语义——一条连接多协议子流，与 iroh 的 per-connection ALPN 刻意不同）；
//! - 状态用 watch（last-value-wins），必达边沿事件用 bounded mpsc，两者不混；
//! - libp2p 类型不出本 crate，上层只见 [`base`] 的 newtype。

mod actor;
mod behaviour;
mod config;
mod dht;
mod endpoint;
mod error;
mod event;
mod lookup;
mod router;
mod rpc;
mod stream;
mod transport;
mod watch;

pub use swarmdrop_net_base as base;
pub use swarmdrop_net_base::{
    Addr, DiscoverySource, NatStatus, NodeAddr, NodeId, PathKind, ProtocolId, SecretKey,
};

pub use config::{DhtConfig, RelayServerConfig};
pub use dht::{Dht, DhtError, DhtKey, DhtRecord};
pub use endpoint::{
    AddrsInfo, BindError, Builder, ConnInfo, Endpoint, InfraRoles, RelayState, presets,
};
pub use error::{AcceptError, ConnectError, Error, OpenError, RpcError};
pub use event::{Events, NetEvent};
pub use lookup::{
    AddressLookup, AddressLookupBuilder, LocalNodeInfo, LookupBuilderFn, LookupError, StaticLookup,
};
pub use router::{ProtocolHandler, Router, RouterBuilder};
pub use rpc::{CallOptions, MAX_RPC_FRAME, Rpc, RpcHandler, RpcMessage, RpcService};
pub use stream::{Direction, P2pStream, StreamLimits};
pub use watch::Watcher;
