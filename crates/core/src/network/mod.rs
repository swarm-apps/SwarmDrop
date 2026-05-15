//! 网络运行时状态和配置。

pub mod config;
pub mod event_loop;
pub mod manager;

pub use manager::{NetManager, SharedNetRefs, TransferRuntime};
pub use swarm_p2p_core::event::NatStatus;

use serde::Serialize;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};

/// 节点运行状态。
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum NodeStatus {
    Running,
    #[default]
    Stopped,
}

/// 网络状态快照。
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub status: NodeStatus,
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    pub peer_id: Option<PeerId>,
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub listen_addrs: Vec<Multiaddr>,
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub nat_status: NatStatus,
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    pub public_addr: Option<Multiaddr>,
    pub connected_peers: usize,
    pub discovered_peers: usize,
    /// Relay 中继是否就绪（至少有一个中继节点已连接）。
    pub relay_ready: bool,
    /// 当前已连接的中继节点 PeerId 列表。
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub relay_peers: Vec<PeerId>,
    /// 是否至少有一个引导节点已连接。
    pub bootstrap_connected: bool,
}
