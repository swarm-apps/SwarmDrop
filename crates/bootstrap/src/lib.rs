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
    Addr, DhtConfig, Endpoint, RelayServerConfig, SecretKey, TransportKind,
    WebTransportCertificateStore, WebTransportConfig, webrtc_direct_addr_from_pem,
};
use tracing::{info, warn};

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
        .webtransport(WebTransportConfig::with_store(config.webtransport_store))
        .idle_timeout(config.idle_timeout)
        .bind()
        .await
        .context("启动 SwarmDrop 网络内核失败")?;

    // WebTransport 的公网地址必须跟着内核走，不能像 TCP/QUIC 那样静态登记 —— 见函数文档。
    // 静态那几条一并交给它：`set_external_addrs` 是整份替换，漏掉就等于撤销。
    spawn_webtransport_external_addr_tracker(
        endpoint.clone(),
        config.external_ip,
        external_addrs.clone(),
    );

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

/// 跟踪 WebTransport 的通告地址，把它们的公网版本登记给内核。
///
/// # 为什么不能像 TCP/QUIC 那样在启动时登记一次
///
/// 那两条的公网地址只由「公网 IP + 端口」决定，恒定不变。WebTransport 的地址里带着
/// **certhash**，而证书每 14 天轮换一次 —— 启动时算出来的那条会在第一次轮换后失效，
/// 而失效的表现是浏览器拨号在 TLS 阶段被拒，日志里看不出与证书有关。
///
/// 所以公网地址跟着内核实际通告的监听地址走：监听地址天然带着**当前正确的** certhash，
/// 这里只把 IP 换成公网 IP。轮换时内核会重新发一轮 `NewAddress`，本任务随之补登记。
///
/// # 为什么整份声明，而不是「发现一条登记一条」
///
/// 登记进去的地址会随 identify 广播给每一个对端，而轮换后的旧地址是**死的**（对端拨过去
/// 在 TLS 阶段被拒），所以必须撤销。判据与两级后果见
/// [`Endpoint::set_external_addrs`]。
///
/// 本任务因此只做一件事：把「此刻应该通告哪些公网地址」整份算出来交给内核。差量、去重、
/// 失败重试全在内核那一份实现里，这里既不记账也不判断增删——**声明式的形态天然对漏采样
/// 免疫**：`watch` 是 last-value-wins，轮换瞬间的中间态大概率被跳过，而每轮都重新声明全集
/// 的话，跳过与否都不影响最终收敛。
///
/// # 静态地址必须一起声明
///
/// TCP / QUIC / WebRTC Direct 那几条的公网地址恒定不变，`Builder::external_addrs` 在 bind
/// 时已经给过一次。但 [`Endpoint::set_external_addrs`] 是**整份替换**：本任务若只声明
/// WebTransport 那几条，第一次调用就会把静态那几条撤销掉 —— relay reservation 的应答随即
/// 失去可拨地址。所以每轮声明的是「静态 ∪ 当前观测」的全集。
fn spawn_webtransport_external_addr_tracker(
    endpoint: Endpoint,
    external_ip: IpAddr,
    static_addrs: Vec<Addr>,
) {
    tokio::spawn(async move {
        let mut addrs = endpoint.watch_addrs();
        // 上一轮**成功声明**的整份内容。两个用途，都不是「记账」：跳过内容相同的重复声明
        // （`watch` 因任何地址变化醒来，包括 relay circuit 上下线），以及据此决定要不要
        // 打日志。失败时不更新它，于是下一轮自然重试 —— 幂等声明不需要记「哪条成功了」。
        let mut declared: Vec<Addr> = Vec::new();

        loop {
            // 筛与改写分开：判据用内核的权威 `Addr::transport()`（它知道
            // `/webtransport` 必须排在 `/quic-v1` 之前），改写用通用的 `Addr::with_ip`。
            // 两者都不是为本调用点特制的 —— 上一版在 crates/net 刻了一个
            // `webtransport_addr_with_ip`，那既重复了判据又在共享层写上了调用方的名字。
            let webtransport: Vec<Addr> = addrs.with(|info| {
                info.listen
                    .iter()
                    .filter(|a| a.transport() == Some(TransportKind::Webtransport))
                    .map(|a| a.with_ip(external_ip))
                    .collect()
            });

            // 静态地址必须一起声明：`set_external_addrs` 是整份替换，漏掉它们就等于撤销。
            // 不在这里去重 —— 内核收到声明后会统一规范化（`0.0.0.0` 展开出的多条改写后
            // 会重合），两处都做就有了两份规范。
            let next: Vec<Addr> = static_addrs.iter().chain(&webtransport).cloned().collect();

            if next != declared {
                match endpoint.set_external_addrs(next.clone()).await {
                    Ok(()) => {
                        for addr in &webtransport {
                            info!(%addr, "已公告 WebTransport 公网地址");
                        }
                        declared = next;
                    }
                    Err(e) => warn!(error = %e, "声明公网地址失败，下轮重试"),
                }
            }

            if addrs.updated().await.is_none() {
                // 内核已关闭。
                break;
            }
        }
    });
}

fn external_addrs(config: &BootstrapConfig) -> Result<Vec<Addr>> {
    let mut addrs: Vec<Addr> = [
        addr(config.external_ip, format!("tcp/{}", config.tcp_port)),
        addr(
            config.external_ip,
            format!("udp/{}/quic-v1", config.quic_port),
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

    #[test]
    fn external_addresses_include_webrtc_certhash() {
        assert!(
            external_addrs(&config())
                .unwrap()
                .iter()
                .any(|addr| addr.to_string().contains("/webrtc-direct/certhash/"))
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

    /// WebTransport 的公网地址**不进** `external_addrs` —— 它的 certhash 每 14 天变一次，
    /// 静态登记的那条会在第一次轮换后失效。改由
    /// `spawn_webtransport_external_addr_tracker` 跟着内核的监听地址走。
    ///
    /// 这条断言存在的意义是防止有人「顺手补齐」：看到 external_addrs 里少了 WebTransport
    /// 就加一条静态的，那正是本设计要避免的东西。
    #[test]
    fn external_addresses_exclude_webtransport() {
        assert!(
            !external_addrs(&config())
                .unwrap()
                .iter()
                .any(|addr| addr.to_string().contains("/webtransport")),
            "WebTransport 公网地址必须由运行时跟踪产生，不能静态登记"
        );
    }
}
