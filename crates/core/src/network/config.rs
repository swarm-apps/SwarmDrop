use serde::{Deserialize, Serialize};
use swarmdrop_net::{Addr, NodeAddr, NodeId};

use super::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};
use super::{BootstrapCandidateManager, DiscoveryMode};

/// SwarmDrop 引导+中继节点身份。
const BOOTSTRAP_NODE_ID: &str = "12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX";

/// 引导节点地址（/ip4/ 格式，所有平台通用——Android 无 DNS transport）。
const BOOTSTRAP_ADDRS: &[&str] = &[
    "/ip4/47.115.172.218/tcp/4001",
    "/ip4/47.115.172.218/udp/4001/quic-v1",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NetworkRuntimeConfig {
    #[serde(default)]
    pub custom_bootstrap_nodes: Vec<String>,
    #[serde(default)]
    pub discovery_mode: DiscoveryMode,
    #[serde(default = "default_true")]
    pub auto_discover_lan_helpers: bool,
    #[serde(default)]
    pub provide_lan_helper: bool,
    /// 公网可达性：允许在已知公网中继上建立 reservation 使本机可被跨网直达。
    ///
    /// 与 discovery_mode 正交——LanOnly 只管"不主动连接内置公网引导"，
    /// 经 LAN Helper 学到的公网中继仍受本开关控制。关闭 = 严格局域网，
    /// 跨网可达仅剩 LAN Helper 转发路径（依赖打洞，可能不可用）。
    #[serde(default = "default_true")]
    pub public_reachability: bool,
}

impl Default for NetworkRuntimeConfig {
    fn default() -> Self {
        Self {
            custom_bootstrap_nodes: Vec::new(),
            discovery_mode: DiscoveryMode::Auto,
            auto_discover_lan_helpers: true,
            provide_lan_helper: false,
            public_reachability: true,
        }
    }
}

const fn default_true() -> bool {
    true
}

/// 从 multiaddr 文本解析 (NodeId, Addr)。
///
/// NodeId 经 [`Addr::p2p_node_id`] 抽取——取末位 P2p 段，circuit 地址上不会错抓中继身份。
fn parse_peer_multiaddr(s: &str) -> Option<(NodeId, Addr)> {
    let addr: Addr = s.parse().ok()?;
    let node_id = addr.p2p_node_id()?;
    Some((node_id, addr))
}

/// 内置引导节点的 [`NodeAddr`]（TCP + QUIC 合并到同一身份）。
fn builtin_bootstrap_node_addr() -> Option<NodeAddr> {
    let node_id = NodeId::from_base58(BOOTSTRAP_NODE_ID).ok()?;
    let addrs: Vec<Addr> = BOOTSTRAP_ADDRS
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    (!addrs.is_empty()).then(|| NodeAddr::with_addrs(node_id, addrs))
}

/// 按 DiscoveryMode 汇总引导节点 [`NodeAddr`]（内置公网 + 用户自定义），供
/// runtime 启动时逐个 `add_infrastructure_peer(bootstrap)`。
pub fn bootstrap_node_addrs(config: &NetworkRuntimeConfig) -> Vec<NodeAddr> {
    let mut peers: Vec<NodeAddr> = Vec::new();
    if matches!(config.discovery_mode, DiscoveryMode::Auto)
        && let Some(node) = builtin_bootstrap_node_addr()
    {
        peers.push(node);
    }
    merge_custom_bootstrap(&mut peers, &config.custom_bootstrap_nodes);
    tracing::info!("Total {} bootstrap peers", peers.len());
    peers
}

/// 把自定义引导节点合并进 peers（同 NodeId 合并地址）。
fn merge_custom_bootstrap(peers: &mut Vec<NodeAddr>, custom: &[String]) {
    for (node_id, addr) in custom.iter().filter_map(|s| parse_peer_multiaddr(s)) {
        match peers.iter_mut().find(|p| p.id == node_id) {
            Some(existing) => {
                if !existing.addrs.contains(&addr) {
                    existing.addrs.push(addr);
                }
            }
            None => peers.push(NodeAddr::with_addrs(node_id, vec![addr])),
        }
    }
}

pub fn create_candidate_manager(config: &NetworkRuntimeConfig) -> BootstrapCandidateManager {
    let mut manager =
        BootstrapCandidateManager::new(config.discovery_mode, config.auto_discover_lan_helpers);
    if matches!(config.discovery_mode, DiscoveryMode::Auto)
        && let Some(node) = builtin_bootstrap_node_addr()
    {
        manager.upsert(
            node.id,
            node.addrs,
            BootstrapCandidateSource::BuiltInPublic,
            CandidateRoles::kad_and_relay(),
            CandidateScope::Public,
        );
    }
    for (node_id, addr) in config
        .custom_bootstrap_nodes
        .iter()
        .filter_map(|s| parse_peer_multiaddr(s))
    {
        manager.upsert(
            node_id,
            vec![addr],
            BootstrapCandidateSource::UserCustom,
            CandidateRoles::kad_and_relay(),
            CandidateScope::Public,
        );
    }
    manager
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::BootstrapCandidateSource;

    #[test]
    fn auto_mode_loads_builtin_candidates() {
        let config = NetworkRuntimeConfig::default();
        let manager = create_candidate_manager(&config);

        assert!(manager.candidate_count() > 0);
        assert!(
            manager
                .source_statuses()
                .iter()
                .any(|status| status.source == BootstrapCandidateSource::BuiltInPublic)
        );
    }

    #[test]
    fn lan_only_skips_builtin_candidates() {
        let config = NetworkRuntimeConfig {
            discovery_mode: DiscoveryMode::LanOnly,
            ..Default::default()
        };
        let manager = create_candidate_manager(&config);

        assert_eq!(manager.candidate_count(), 0);
    }

    #[test]
    fn custom_candidate_is_loaded_in_lan_only_mode() {
        let custom =
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX";
        let config = NetworkRuntimeConfig {
            discovery_mode: DiscoveryMode::LanOnly,
            custom_bootstrap_nodes: vec![custom.to_string()],
            ..Default::default()
        };
        let manager = create_candidate_manager(&config);

        assert_eq!(manager.candidate_count(), 1);
        assert!(
            manager
                .source_statuses()
                .iter()
                .any(|status| status.source == BootstrapCandidateSource::UserCustom)
        );
    }

    #[test]
    fn bootstrap_node_addrs_merges_tcp_and_quic_into_one_peer() {
        let peers = bootstrap_node_addrs(&NetworkRuntimeConfig::default());
        assert_eq!(peers.len(), 1, "内置 TCP+QUIC 应合并到同一身份");
        assert_eq!(peers[0].addrs.len(), 2);
    }
}
