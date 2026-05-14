use std::time::Duration;
use swarm_p2p_core::{
    libp2p::{multiaddr::Protocol, Multiaddr, PeerId},
    NodeConfig,
};

/// SwarmDrop 引导+中继节点
///
/// 使用 /ip4/ 格式，所有平台通用（Android 无 DNS transport）。
const BOOTSTRAP_NODES: &[&str] = &[
    "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
    "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
];

/// 解析 Multiaddr 字符串列表为 (PeerId, Multiaddr) 对
fn parse_multiaddrs(addrs: &[impl AsRef<str>]) -> Vec<(PeerId, Multiaddr)> {
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
///
/// `custom_bootstrap_nodes` — 用户自定义的额外引导节点地址，与默认节点合并
pub fn create_node_config(agent_version: String, custom_bootstrap_nodes: &[String]) -> NodeConfig {
    let mut bootstrap_peers = parse_multiaddrs(BOOTSTRAP_NODES);

    // 合并自定义引导节点
    if !custom_bootstrap_nodes.is_empty() {
        let custom_peers = parse_multiaddrs(custom_bootstrap_nodes);
        tracing::info!("Parsed {} custom bootstrap peers", custom_peers.len());
        bootstrap_peers.extend(custom_peers);
    }

    tracing::info!("Total {} bootstrap peers", bootstrap_peers.len());

    NodeConfig::new("/swarmdrop/1.0.0", agent_version)
        .with_mdns(true)
        .with_relay_client(true)
        .with_dcutr(true)
        .with_autonat(true)
        .with_req_resp_timeout(Duration::from_secs(180))
        .with_bootstrap_peers(bootstrap_peers)
}
