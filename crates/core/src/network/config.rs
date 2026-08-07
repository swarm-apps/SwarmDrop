use serde::{Deserialize, Serialize};
use swarmdrop_net::{Addr, NodeAddr, NodeId};

use super::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};
use super::{BootstrapCandidateManager, DiscoveryMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NetworkRuntimeConfig {
    /// 由当前 host 提供的引导/中继节点完整地址。
    ///
    /// 公共基础设施是桌面、移动和 Web 各自的部署策略，不属于跨平台核心；各端可按自身
    /// transport 能力提供不同的 TCP、QUIC、WebSocket 或 WebRTC Direct 地址。
    #[serde(default)]
    pub bootstrap_nodes: Vec<String>,
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
            bootstrap_nodes: Vec::new(),
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

/// 汇总 host 提供的引导节点 [`NodeAddr`]，供 runtime 启动时逐个注册为基础设施节点。
pub fn bootstrap_node_addrs(config: &NetworkRuntimeConfig) -> Vec<NodeAddr> {
    let mut peers: Vec<NodeAddr> = Vec::new();
    merge_bootstrap_nodes(&mut peers, &config.bootstrap_nodes);
    tracing::info!("Total {} bootstrap peers", peers.len());
    peers
}

/// 把 host 提供的引导节点合并进 peers（同 NodeId 合并地址）。
fn merge_bootstrap_nodes(peers: &mut Vec<NodeAddr>, nodes: &[String]) {
    for (node_id, addr) in nodes.iter().filter_map(|s| parse_peer_multiaddr(s)) {
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
    for (node_id, addr) in config
        .bootstrap_nodes
        .iter()
        .filter_map(|s| parse_peer_multiaddr(s))
    {
        manager.upsert(
            node_id,
            vec![addr],
            BootstrapCandidateSource::HostConfigured,
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
    fn default_config_has_no_public_candidates() {
        let config = NetworkRuntimeConfig::default();
        let manager = create_candidate_manager(&config);

        assert_eq!(manager.candidate_count(), 0);
    }

    #[test]
    fn host_configured_candidate_is_loaded_in_lan_only_mode() {
        let custom =
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX";
        let config = NetworkRuntimeConfig {
            discovery_mode: DiscoveryMode::LanOnly,
            bootstrap_nodes: vec![custom.to_string()],
            ..Default::default()
        };
        let manager = create_candidate_manager(&config);

        assert_eq!(manager.candidate_count(), 1);
        assert!(
            manager
                .source_statuses()
                .iter()
                .any(|status| status.source == BootstrapCandidateSource::HostConfigured)
        );
    }

    #[test]
    fn bootstrap_node_addrs_merges_host_addresses_for_one_peer() {
        let config = NetworkRuntimeConfig {
            bootstrap_nodes: vec![
                "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX".to_string(),
                "/ip4/127.0.0.1/udp/4001/quic-v1/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX".to_string(),
            ],
            ..Default::default()
        };
        let peers = bootstrap_node_addrs(&config);
        assert_eq!(peers.len(), 1, "同一 host 节点的 TCP+QUIC 地址应合并");
        assert_eq!(peers[0].addrs.len(), 2);
    }
}
