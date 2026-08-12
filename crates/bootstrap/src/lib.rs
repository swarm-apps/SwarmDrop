//! SwarmDrop 公网引导 + relay 节点。
//!
//! 该组合根只装配基础设施能力，网络事件循环、DHT、Relay 与 WebRTC Direct
//! 均复用 [`swarmdrop_net::Endpoint`]，避免与客户端内核分叉。

pub mod util;

use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use swarmdrop_net::{
    Addr, DhtConfig, Endpoint, RelayServerConfig, SecretKey, WebTransportCertificateStore,
    WebTransportConfig,
};
use tracing::info;

const IDENTIFY_PROTOCOL: &str = "/swarmdrop/2.0.0";
/// 引导 + relay 节点的运行配置。
#[derive(Clone)]
pub struct BootstrapConfig {
    pub secret_key: SecretKey,
    pub webrtc_certificate_pem: String,
    pub listen_ip: IpAddr,
    pub external_ip: IpAddr,
    pub tcp_port: u16,
    pub quic_port: u16,
    pub webrtc_port: u16,
    /// WebTransport 监听端口。**独占**，不与 QUIC 共用 —— wtransport 不接受已绑定的
    /// socket，而 QUIC 用 libp2p TLS 扩展证书、WebTransport 用普通自签名证书，
    /// rustls 配置本就不同。
    pub webtransport_port: u16,
    /// WebTransport 证书对的持久化端口。
    pub webtransport_store: Arc<dyn WebTransportCertificateStore>,
    pub idle_timeout: Duration,
    pub relay_limits: RelayServerConfig,
}

impl std::fmt::Debug for BootstrapConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BootstrapConfig")
            .field("listen_ip", &self.listen_ip)
            .field("external_ip", &self.external_ip)
            .field("tcp_port", &self.tcp_port)
            .field("quic_port", &self.quic_port)
            .field("webrtc_port", &self.webrtc_port)
            .field("webtransport_port", &self.webtransport_port)
            .field("idle_timeout", &self.idle_timeout)
            .field("relay_limits", &self.relay_limits)
            .finish_non_exhaustive()
    }
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
        .external_addrs(external_addrs)
        // 其余公网地址由内核从监听地址映射：WebTransport 与 WebRTC Direct 的地址里带
        // certhash，跟着监听地址走才拿得到当前正确的那个（判据见 `Builder::external_ip`）。
        .external_ip(config.external_ip)
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
        .webtransport(WebTransportConfig::with_store(config.webtransport_store))
        .idle_timeout(config.idle_timeout)
        .bind()
        .await
        .context("启动 SwarmDrop 网络内核失败")?;

    info!(node_id = %endpoint.node_id(), "Bootstrap + Relay 节点已启动");
    spawn_external_addr_logger(endpoint.clone());
    info!("等待连接；按 Ctrl+C 或发送 SIGTERM 关闭");

    util::signal::shutdown_signal().await;
    info!("正在关闭 Bootstrap + Relay 节点");
    endpoint.close().await;
    Ok(())
}

/// 把内核当前通告的公网地址打进日志，随其变化重打。**只做日志，不参与任何决策。**
///
/// 这是运维面而非协议面的需要：这台机器的全部职责就是「在几个已知地址上可被拨到」，
/// 而其中两条带 certhash（WebTransport 每 14 天还会换一次），部署后没有别的办法确认它们
/// 长成什么样。默认 filter 是 `info`，所以打在 info 上。
///
/// ⚠️ **不要让它变回「跟踪并声明」**：地址的差量、去重、撤销全部住在内核
/// （`Builder::external_ip` + `Endpoint::set_external_addrs`），这里再记一份账就有了两个
/// 事实源——而两者不一致时的症状是「某条地址悄悄不再被通告」，日志上完全看不出来。
fn spawn_external_addr_logger(endpoint: Endpoint) {
    tokio::spawn(async move {
        let mut addrs = endpoint.watch_addrs();
        // `watch` 是 last-value-wins，中间态可能被跳过——对日志无所谓，打的始终是当前全集。
        let mut last: Vec<Addr> = Vec::new();
        loop {
            let external = addrs.with(|info| info.external.clone());
            if external != last {
                for addr in &external {
                    info!(%addr, "已公告公网地址");
                }
                last = external;
            }
            if addrs.updated().await.is_none() {
                // 内核已关闭。
                break;
            }
        }
    });
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
            format!("udp/{}/webrtc-direct", config.webrtc_port),
        ),
        addr(
            config.listen_ip,
            format!("udp/{}/quic-v1/webtransport", config.webtransport_port),
        ),
    ]
    .into_iter()
    .collect()
}

/// 启动即可声明的公网地址 —— **只有** TCP / QUIC 这两条。
///
/// 它们的公网形态只由「公网 IP + 端口」决定，不用等监听结果就能算出来，所以在这里预先
/// 声明一次，覆盖 bind 返回到第一批 `NewListenAddr` 抵达之间的窗口：那期间来的
/// reservation 请求若拿不到任何可拨地址，客户端会以 `NoAddressesInReservation` 拒掉整个
/// reservation（本仓实测踩过）。内核的 `external_ip` 映射随后会算出同样的两条，重合去重，
/// 这份预声明因此是纯保险、无副作用。
///
/// **带 certhash 的两条（WebRTC Direct / WebTransport）刻意不在这里。** 静态算它们需要
/// 另起一条「从证书派生 certhash」的路径，与传输实际使用的那条一旦漂移，症状是浏览器在
/// TLS 阶段被拒、而日志里看不出与证书有关；WebTransport 更是每 14 天轮换一次，静态那条
/// 第一次轮换后就成了死地址。两条都交给 `external_ip` 从**监听地址本身**取——那里的
/// certhash 按定义就是当前正确的那个。
fn external_addrs(config: &BootstrapConfig) -> Result<Vec<Addr>> {
    [
        addr(config.external_ip, format!("tcp/{}", config.tcp_port)),
        addr(
            config.external_ip,
            format!("udp/{}/quic-v1", config.quic_port),
        ),
    ]
    .into_iter()
    .collect()
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

    fn config() -> BootstrapConfig {
        BootstrapConfig {
            secret_key: SecretKey::generate(),
            webrtc_certificate_pem: swarmdrop_net::generate_webrtc_certificate_pem().unwrap(),
            listen_ip: "0.0.0.0".parse().unwrap(),
            external_ip: "203.0.113.10".parse().unwrap(),
            tcp_port: 4001,
            quic_port: 4001,
            webrtc_port: 4003,
            webtransport_port: 4004,
            webtransport_store: Arc::new(
                swarmdrop_net::WebTransportMemoryCertificateStore::default(),
            ),
            idle_timeout: Duration::from_secs(120),
            relay_limits: RelayServerConfig::default(),
        }
    }

    /// 启动即声明的**只有** TCP / QUIC 两条。
    ///
    /// 这条挡的是「顺手补齐」：看到清单里少了 webrtc-direct / webtransport 就静态加一条。
    /// 那需要另起一条从证书派生 certhash 的路径，而与传输实际使用的那条一旦漂移，症状是
    /// 浏览器在 TLS 阶段被拒、日志里毫无线索；WebTransport 更是每 14 天轮换一次，静态那条
    /// 第一次轮换后就成了死地址。两条都由内核的 `external_ip` 从监听地址映射。
    #[test]
    fn statically_declared_addresses_are_only_the_certhash_free_ones() {
        let addrs: Vec<String> = external_addrs(&config())
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect();

        assert_eq!(
            addrs,
            vec![
                "/ip4/203.0.113.10/tcp/4001".to_string(),
                "/ip4/203.0.113.10/udp/4001/quic-v1".to_string(),
            ]
        );
    }

    /// 四条监听地址一条都不能少，且 WebTransport 独占自己的端口。
    ///
    /// 与 4003 并存是刻意的：两条浏览器入口同时提供，可以对比吞吐后再决定是否下线
    /// webrtc-direct（在那之前它仍是浏览器唯一验证过的入口）。
    #[test]
    fn listens_on_webtransport_alongside_webrtc_direct() {
        let addrs: Vec<String> = listen_addrs(&config())
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect();

        assert!(addrs.iter().any(|a| a == "/ip4/0.0.0.0/tcp/4001"));
        assert!(addrs.iter().any(|a| a == "/ip4/0.0.0.0/udp/4001/quic-v1"));
        assert!(
            addrs
                .iter()
                .any(|a| a == "/ip4/0.0.0.0/udp/4003/webrtc-direct")
        );
        assert!(
            addrs
                .iter()
                .any(|a| a == "/ip4/0.0.0.0/udp/4004/quic-v1/webtransport"),
            "实得 {addrs:?}"
        );
    }

    /// 静态声明的那几条，内容必须与「监听地址换上公网 IP」得到的一致。
    ///
    /// 两者会被内核取并集：不一致就等于凭空多通告一条从未监听过的地址，而它照样会随
    /// identify 广播出去。这条把「预声明是纯保险」这句话变成可检查的事实。
    #[test]
    fn statically_declared_addresses_agree_with_the_public_ip_mapping() {
        let config = config();
        let mapped: Vec<String> = listen_addrs(&config)
            .unwrap()
            .iter()
            .map(|a| a.with_ip(config.external_ip).to_string())
            .collect();

        for declared in external_addrs(&config).unwrap() {
            assert!(
                mapped.contains(&declared.to_string()),
                "{declared} 不在监听地址的公网映射里：{mapped:?}"
            );
        }
    }
}
