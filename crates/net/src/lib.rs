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
mod addrset;
mod behaviour;
// 文件持久化只在 native 有意义（浏览器不监听 WebTransport，也没有文件系统）。
#[cfg(not(wasm_browser))]
mod cert_store;
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
    Addr, AddrParseError, DiscoverySource, NatStatus, NodeAddr, NodeId, PathKind, ProtocolId,
    SecretKey, TransportKind,
};

pub use config::{DhtConfig, RelayServerConfig, WebRtcP2pConfig};
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

/// 生成可持久化的 WebRTC Direct 证书 PEM（含私钥）。
///
/// 调用方必须把完整 PEM 放入安全存储后在每次启动时复用，不能仅保存密钥再重建。
#[cfg(not(wasm_browser))]
pub fn generate_webrtc_certificate_pem() -> Result<String, String> {
    webrtc_p2p::Certificate::generate()
        .map_err(|error| error.to_string())
        .map(|certificate| certificate.serialize_pem())
}

// ⚠️ 这里曾有一个 `webrtc_direct_addr_from_pem`（从持久化 PEM 预先派生可公告的
// webrtc-direct 公网地址），2026-08-12 删除，**不要加回来**。
//
// 它是「从证书算 certhash」的第二条路径，与传输启动时实际使用的那条并行存在；两者一旦
// 漂移，症状是浏览器在 TLS 阶段被拒、而本机日志毫无线索。它唯一的消费方（bootstrap）已
// 改用 [`Builder::external_ip`](endpoint::builder::Builder::external_ip)——那条从**监听
// 地址本身**取 certhash，按定义不可能与传输不一致，并且顺带覆盖了 WebTransport 那种
// certhash 会周期性轮换、静态派生根本算不对的情形。

/// WebTransport 配置。**两个 target 都在**——它是上层唯一需要认识的类型，
/// 证书端口藏在它后面（判据见该类型的文档）。
pub use config::WebTransportConfig;

/// WebTransport 的证书持久化端口与其两个实现。**native only**：浏览器不监听、
/// 没有服务端证书可存，`webtransport_p2p` 在那边也不在依赖树里。
///
/// 直接转出上游的 trait 而不做镜像 —— `crates/net` 正是本仓允许写 cfg 的平台边界层，
/// 在这里挡住比在上层每个组合根挡一次便宜。
#[cfg(not(wasm_browser))]
pub use cert_store::WebTransportFileCertificateStore;
#[cfg(not(wasm_browser))]
pub use webtransport_p2p::{
    CertificateStore as WebTransportCertificateStore,
    MemoryCertificateStore as WebTransportMemoryCertificateStore,
    StoreError as WebTransportStoreError,
};
