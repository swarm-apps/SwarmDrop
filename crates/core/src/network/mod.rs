//! 网络运行时状态和配置。

pub mod candidates;
pub mod config;
pub mod event_loop;
pub mod manager;

pub use candidates::{
    BootstrapCandidate, BootstrapCandidateManager, BootstrapCandidateSource, CandidateHealth,
    CandidateRoles, CandidateScope, CandidateSourceStatus,
};
pub use config::NetworkRuntimeConfig;
pub use manager::{NetManager, SharedNetRefs, TransferRuntime};
pub use swarmdrop_net::NatStatus;

use serde::{Deserialize, Serialize};
use swarmdrop_net::{Addr, NodeId};

/// 节点运行状态。
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum NodeStatus {
    Running,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryMode {
    #[default]
    Auto,
    LanOnly,
}

/// 网络状态快照。
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub status: NodeStatus,
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    pub peer_id: Option<NodeId>,
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub listen_addrs: Vec<Addr>,
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub nat_status: NatStatus,
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    pub public_addr: Option<Addr>,
    pub connected_peers: usize,
    pub discovered_peers: usize,
    /// Relay 中继是否就绪（至少有一个中继节点已连接）。
    pub relay_ready: bool,
    /// 公网可达：持有公网范围中继的活跃 reservation，或已确认公网直达地址。
    /// false = 仅局域网可达（跨网设备无法直接访问本机）。
    pub public_reachable: bool,
    /// 公网可达性设置的回显（host 侧检测"设置已变更需重启"用）。
    pub public_reachability_enabled: bool,
    /// 当前已连接的中继节点 NodeId 列表。
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub relay_peers: Vec<NodeId>,
    /// 是否至少有一个引导节点已连接。
    pub bootstrap_connected: bool,
    /// 当前发现模式。
    pub discovery_mode: DiscoveryMode,
    /// 是否自动发现局域网协助节点。
    pub auto_discover_lan_helpers: bool,
    /// 本设备是否配置为提供局域网协助能力。
    pub local_lan_helper_enabled: bool,
    /// 本设备当前是否正在作为局域网协助节点运行。
    pub local_lan_helper_running: bool,
    /// Relay Server 是否启用。
    pub relay_server_enabled: bool,
    /// LAN Helper 可公告地址。
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub lan_helper_advertised_addrs: Vec<Addr>,
    /// 已发现的局域网协助节点数量。
    pub lan_helper_count: usize,
    /// 候选总数。
    pub bootstrap_candidate_count: usize,
    /// 按来源聚合的候选数量。
    pub candidate_sources: Vec<CandidateSourceStatus>,
    /// 当前 relay peer 的候选来源。
    pub relay_source: Option<BootstrapCandidateSource>,
}
