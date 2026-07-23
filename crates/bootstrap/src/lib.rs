//! SwarmDrop 公网引导 + relay 节点。
//!
//! 该组合根只装配基础设施能力，网络事件循环、DHT、Relay 与 WebRTC Direct
//! 均复用 [`swarmdrop_net::Endpoint`]，避免与客户端内核分叉。

pub mod util;

use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::time::Duration;

use anyhow::{Context, Result};
use swarmdrop_net::{
    Addr, DhtConfig, Endpoint, RelayServerConfig, SecretKey, webrtc_direct_addr_from_pem,
};
use tracing::info;

const IDENTIFY_PROTOCOL: &str = "/swarmdrop/2.0.0";
/// 引导 + relay 节点的运行配置。
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub secret_key: SecretKey,
    pub webrtc_certificate_pem: String,
    pub listen_ip: IpAddr,
    pub external_ip: IpAddr,
    pub tcp_port: u16,
    pub quic_port: u16,
    pub websocket_port: u16,
    pub webrtc_port: u16,
    pub idle_timeout: Duration,
    pub relay_limits: RelayServerConfig,
}

/// 启动引导 + relay 服务，直至接到关闭信号。
pub async fn run(config: BootstrapConfig) -> Result<()> {
    let listen_addrs = listen_addrs(&config)?;
    let external_addrs = external_addrs(&config)?;
    let endpoint = Endpoint::builder()
        .secret_key(config.secret_key)
        .identify_protocol(IDENTIFY_PROTOCOL)
        .agent_version(format!("swarm-bootstrap/{}", env!("CARGO_PKG_VERSION")))
        .listen(listen_addrs)
        .external_addrs(external_addrs.clone())
        .dht(DhtConfig {
            query_timeout: Duration::from_secs(60),
            record_ttl: Duration::from_secs(2 * 3600),
            replication_factor: NonZeroUsize::new(20).expect("20 is non-zero"),
            publication_interval: Duration::from_secs(3600),
            server_mode: true,
        })
        .relay_client(false)
        .relay_server(config.relay_limits)
        .autonat_server(true)
        .webrtc_certificate(config.webrtc_certificate_pem)
        .idle_timeout(config.idle_timeout)
        .bind()
        .await
        .context("启动 SwarmDrop 网络内核失败")?;

    info!(node_id = %endpoint.node_id(), "Bootstrap + Relay 节点已启动");
    for addr in external_addrs {
        info!(%addr, "已公告公网地址");
    }
    info!("等待连接；按 Ctrl+C 或发送 SIGTERM 关闭");

    util::signal::shutdown_signal().await;
    info!("正在关闭 Bootstrap + Relay 节点");
    endpoint.close().await;
    Ok(())
}

fn listen_addrs(config: &BootstrapConfig) -> Result<Vec<Addr>> {
    [
        addr(config.listen_ip, format!("tcp/{}", config.tcp_port)),
        addr(
            config.listen_ip,
            format!("udp/{}/quic-v1", config.quic_port),
        ),
        addr(
            config.listen_ip,
            format!("tcp/{}/ws", config.websocket_port),
        ),
        addr(
            config.listen_ip,
            format!("udp/{}/webrtc-direct", config.webrtc_port),
        ),
    ]
    .into_iter()
    .collect()
}

fn external_addrs(config: &BootstrapConfig) -> Result<Vec<Addr>> {
    let mut addrs: Vec<Addr> = [
        addr(config.external_ip, format!("tcp/{}", config.tcp_port)),
        addr(
            config.external_ip,
            format!("udp/{}/quic-v1", config.quic_port),
        ),
        addr(
            config.external_ip,
            format!("tcp/{}/ws", config.websocket_port),
        ),
    ]
    .into_iter()
    .collect::<Result<_>>()?;
    addrs.push(
        webrtc_direct_addr_from_pem(
            config.external_ip,
            config.webrtc_port,
            &config.webrtc_certificate_pem,
        )
        .map_err(anyhow::Error::msg)
        .context("从 WebRTC Direct 证书构造公网地址失败")?,
    );
    Ok(addrs)
}

fn addr(ip: IpAddr, suffix: String) -> Result<Addr> {
    let protocol = match ip {
        IpAddr::V4(_) => "ip4",
        IpAddr::V6(_) => "ip6",
    };
    format!("/{protocol}/{ip}/{suffix}")
        .parse()
        .context("生成监听地址失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_addresses_include_webrtc_certhash() {
        let config = BootstrapConfig {
            secret_key: SecretKey::generate(),
            webrtc_certificate_pem: swarmdrop_net::generate_webrtc_certificate_pem().unwrap(),
            listen_ip: "0.0.0.0".parse().unwrap(),
            external_ip: "203.0.113.10".parse().unwrap(),
            tcp_port: 4001,
            quic_port: 4001,
            websocket_port: 4002,
            webrtc_port: 4003,
            idle_timeout: Duration::from_secs(120),
            relay_limits: RelayServerConfig::default(),
        };
        assert!(
            external_addrs(&config)
                .unwrap()
                .iter()
                .any(|addr| addr.to_string().contains("/webrtc-direct/certhash/"))
        );
    }
}
