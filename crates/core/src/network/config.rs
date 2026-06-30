use std::time::Duration;

use serde::{Deserialize, Serialize};
use swarm_p2p_core::{
    InfrastructureMode, LanHelperConfig, NodeConfig,
    libp2p::{Multiaddr, PeerId, StreamProtocol, multiaddr::Protocol},
};

use super::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};
use super::{BootstrapCandidateManager, DiscoveryMode};
use crate::transfer::wire::data_frame::TRANSFER_DATA_PROTOCOL;

/// SwarmDrop 引导+中继节点
///
/// 使用 /ip4/ 格式，所有平台通用（Android 无 DNS transport）。
const BOOTSTRAP_NODES: &[&str] = &[
    "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
    "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
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
}

impl Default for NetworkRuntimeConfig {
    fn default() -> Self {
        Self {
            custom_bootstrap_nodes: Vec::new(),
            discovery_mode: DiscoveryMode::Auto,
            auto_discover_lan_helpers: true,
            provide_lan_helper: false,
        }
    }
}

const fn default_true() -> bool {
    true
}

/// 解析 Multiaddr 字符串列表为 (PeerId, Multiaddr) 对
pub fn parse_multiaddrs(addrs: &[impl AsRef<str>]) -> Vec<(PeerId, Multiaddr)> {
    addrs
        .iter()
        .filter_map(|s| {
            let addr: Multiaddr = s.as_ref().parse().ok()?;
            let peer_id = addr.iter().find_map(|p| match p {
                Protocol::P2p(id) => Some(id),
                _ => None,
            })?;
            Some((peer_id, addr))
        })
        .collect()
}

/// 创建 P2P 节点配置
pub fn create_node_config(agent_version: String, config: &NetworkRuntimeConfig) -> NodeConfig {
    let mut bootstrap_peers = match config.discovery_mode {
        DiscoveryMode::Auto => parse_multiaddrs(BOOTSTRAP_NODES),
        DiscoveryMode::LanOnly => Vec::new(),
    };

    // 合并自定义引导节点
    if !config.custom_bootstrap_nodes.is_empty() {
        let custom_peers = parse_multiaddrs(&config.custom_bootstrap_nodes);
        tracing::info!("Parsed {} custom bootstrap peers", custom_peers.len());
        bootstrap_peers.extend(custom_peers);
    }

    tracing::info!("Total {} bootstrap peers", bootstrap_peers.len());

    let mut node_config = NodeConfig::new("/swarmdrop/1.0.0", agent_version)
        .with_mdns(true)
        .with_relay_client(true)
        .with_dcutr(true)
        .with_autonat(true)
        .with_req_resp_timeout(Duration::from_secs(180))
        .with_data_channel_protocols(vec![StreamProtocol::new(TRANSFER_DATA_PROTOCOL)])
        .with_bootstrap_peers(bootstrap_peers);

    if config.provide_lan_helper {
        node_config = node_config
            .with_infrastructure_mode(InfrastructureMode::LanHelper(LanHelperConfig::default()));
    }

    node_config
}

pub fn create_candidate_manager(config: &NetworkRuntimeConfig) -> BootstrapCandidateManager {
    let mut manager =
        BootstrapCandidateManager::new(config.discovery_mode, config.auto_discover_lan_helpers);
    if matches!(config.discovery_mode, DiscoveryMode::Auto) {
        for (peer_id, addr) in parse_multiaddrs(BOOTSTRAP_NODES) {
            manager.upsert(
                peer_id,
                vec![addr],
                BootstrapCandidateSource::BuiltInPublic,
                CandidateRoles::kad_and_relay(),
                CandidateScope::Public,
            );
        }
    }
    for (peer_id, addr) in parse_multiaddrs(&config.custom_bootstrap_nodes) {
        manager.upsert(
            peer_id,
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
}
