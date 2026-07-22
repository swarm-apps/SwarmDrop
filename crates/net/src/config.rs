//! 内核配置类型（Builder 的字段载体）。

use std::num::NonZeroUsize;
use std::time::Duration;

use swarmdrop_net_base::Addr;

use crate::stream::StreamLimits;

/// Kademlia DHT 配置。`Builder::dht(DhtConfig::default())` 启用。
#[derive(Debug, Clone)]
pub struct DhtConfig {
    /// 查询超时（网络差时可适当增加）。
    pub query_timeout: Duration,
    /// record 生存时间（分享码/在线宣告的服务器侧过期）。
    pub record_ttl: Duration,
    /// 复制因子（record 存到 N 个最近节点）。
    pub replication_factor: NonZeroUsize,
    /// 定期重发布间隔（保持 record 有效）。
    pub publication_interval: Duration,
    /// 强制 Kad Server 模式。默认由 AutoNAT 判定（公网可达才切 Server）；
    /// 已知可达的场景（测试、引导节点、LanHelper）设 true。
    pub server_mode: bool,
}

impl Default for DhtConfig {
    fn default() -> Self {
        Self {
            query_timeout: Duration::from_secs(60),
            record_ttl: Duration::from_secs(3600),
            replication_factor: NonZeroUsize::new(3).expect("non-zero"),
            publication_interval: Duration::from_secs(3600),
            server_mode: false,
        }
    }
}

/// 中继服务端配置（LanHelper：本机兼任 relay server 给局域网/受限设备转发）。
#[derive(Debug, Clone)]
pub struct RelayServerConfig {
    /// 最大 reservation 数。
    pub max_reservations: usize,
    /// 每 peer 最大 reservation 数。
    pub max_reservations_per_peer: usize,
    /// reservation 时长。
    pub reservation_duration: Duration,
    /// 最大并发 circuit 数。
    pub max_circuits: usize,
    /// 每 peer 最大并发 circuit 数。
    pub max_circuits_per_peer: usize,
    /// 单个 circuit 最长时长。
    pub max_circuit_duration: Duration,
    /// 单个 circuit 最大转发字节（文件中转场景要够大，默认不限）。
    pub max_circuit_bytes: u64,
    /// 把私网监听地址登记为 external（identify 广播给局域网对端，
    /// 让它们发现「这里有个 helper」）。
    pub announce_private_addrs: bool,
}

impl Default for RelayServerConfig {
    fn default() -> Self {
        Self {
            max_reservations: 32,
            max_reservations_per_peer: 4,
            reservation_duration: Duration::from_secs(3600),
            max_circuits: 16,
            max_circuits_per_peer: 4,
            max_circuit_duration: Duration::from_secs(6 * 3600),
            // 文件传输经中继不掐断（旧栈 LanHelper 同值）
            max_circuit_bytes: u64::MAX,
            announce_private_addrs: true,
        }
    }
}

/// 内部装配配置（Builder 收集、bind 时消费）。
#[derive(Clone)]
pub(crate) struct EndpointConfig {
    /// identify 的 protocol_version。默认是中立的内核值——业务协议契约
    /// （如 `/swarmdrop/2.0.0`）由上层经 `Builder::identify_protocol` 显式注入。
    pub identify_protocol: String,
    /// identify 的 agent_version（可含设备能力信息）。
    pub agent_version: String,
    pub ping_interval: Duration,
    pub ping_timeout: Duration,
    /// 空闲连接回收时限（keep_alive 白名单内的 peer 豁免）。
    pub idle_timeout: Duration,
    pub dht: Option<DhtConfig>,
    /// mDNS 局域网发现（wasm 下忽略——behaviour 字段编译期不存在）。
    pub mdns: bool,
    pub autonat: bool,
    pub dcutr: bool,
    pub relay_client: bool,
    /// 中继服务端（LanHelper；wasm 下忽略——behaviour 字段编译期不存在）。
    pub relay_server: Option<RelayServerConfig>,
    /// webrtc-direct 的持久化证书（PEM）。**certhash 进分享地址——证书变则
    /// 分享出去的地址全部失效**，生产必须注入持久化证书（keychain/数据目录）；
    /// `None` 时每次随机生成（仅测试/临时场景可接受）。native only。
    pub webrtc_cert_pem: Option<String>,
    /// 监听地址（wasm 下必须为空——浏览器不能 listen 本地 socket，
    /// circuit listen 由 `ensure_relay_reservation` 触发）。
    pub listen: Vec<Addr>,
    pub stream_limits: StreamLimits,
    /// connect 的整体超时。
    pub connect_timeout: Duration,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            identify_protocol: "/swarmdrop-net/0.1".into(),
            agent_version: "swarmdrop-net/0.1".into(),
            ping_interval: Duration::from_secs(30),
            ping_timeout: Duration::from_secs(20),
            idle_timeout: Duration::from_secs(60),
            dht: None,
            mdns: false,
            autonat: false,
            dcutr: false,
            relay_client: true,
            relay_server: None,
            webrtc_cert_pem: None,
            listen: Vec::new(),
            stream_limits: StreamLimits::default(),
            connect_timeout: Duration::from_secs(30),
        }
    }
}

impl std::fmt::Debug for EndpointConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EndpointConfig")
            .field("identify_protocol", &self.identify_protocol)
            .field("agent_version", &self.agent_version)
            .field("ping_interval", &self.ping_interval)
            .field("ping_timeout", &self.ping_timeout)
            .field("idle_timeout", &self.idle_timeout)
            .field("dht", &self.dht)
            .field("mdns", &self.mdns)
            .field("autonat", &self.autonat)
            .field("dcutr", &self.dcutr)
            .field("relay_client", &self.relay_client)
            .field("relay_server", &self.relay_server)
            // 绝不打印证书私钥材料，只标注是否已注入持久化证书
            .field(
                "webrtc_cert_pem",
                &self.webrtc_cert_pem.as_ref().map(|_| "<redacted>"),
            )
            .field("listen", &self.listen)
            .field("stream_limits", &self.stream_limits)
            .field("connect_timeout", &self.connect_timeout)
            .finish()
    }
}
