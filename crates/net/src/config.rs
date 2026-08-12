//! 内核配置类型（Builder 的字段载体）。

use std::num::NonZeroUsize;
// 只有 native 用得到：`Arc<dyn CertificateStore>` 的三处使用都在 cfg 门控下，
// 无条件 import 在 wasm 下是 unused —— 而那条 CI job 跑的是 `-D warnings`。
#[cfg(not(wasm_browser))]
use std::sync::Arc;
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

/// WebRTC 打洞传输配置（`Builder::webrtc_p2p(..)` 启用）。
///
/// 它与 webrtc-direct 是**两个不同的传输**：后者要求目标地址已可达（ICE-lite，不打洞），
/// 前者让双方都不可达的节点经 relay 换信令后打洞直连。两者可同时开启。
///
/// 这里刻意不复用 `webrtc_p2p::Config`——内核不让第三方类型穿透到上层 API
/// （与「libp2p 类型不出内核」同一条约束），转换在 `transport.rs` 就地完成。
#[derive(Debug, Clone)]
pub struct WebRtcP2pConfig {
    /// STUN 服务器。
    ///
    /// **不是可选优化**：没有 STUN 就只有 host candidate，跨 NAT 必然打不通
    /// （浏览器无法经 identify 发现自己的公网地址——每条 WebRTC 连接用新端口）。
    /// 默认取 `webrtc_p2p` 的公共 STUN；国内网络可换成可达的服务器。
    pub stun_servers: Vec<String>,
    /// 信令交换整体超时（开流 → offer → answer → ICE 收敛）。
    ///
    /// 超时后 dial 失败，由上层决定是否退回 relay 中转——spec 步骤 8 把该策略留给应用。
    pub signaling_timeout: Duration,
}

impl Default for WebRtcP2pConfig {
    fn default() -> Self {
        let defaults = webrtc_p2p::Config::default();
        Self {
            stun_servers: defaults.stun_servers().to_vec(),
            signaling_timeout: defaults.signaling_timeout(),
        }
    }
}

/// WebTransport 传输的配置。**native only**。
///
/// # 为什么「启用」与「证书持久化」是两件事
///
/// 拨号方不需要服务端证书 —— 它只验证对端的。上一版把两者合成一个
/// `Option<store>`，于是纯拨号方必须伪造一个永不被读的存储，而「该持久化却没持久化」
/// 那条警告从此永远不响：真正的错误配置变得不可检测。
///
/// 与 [`WebRtcP2pConfig`] 同体例：内核不把第三方 crate 的配置类型直接暴露给上层。
///
/// # 为什么这个类型两个 target 都在，而证书端口只在 native
///
/// 证书端口是 `webtransport_p2p::CertificateStore`（经 `swarmdrop_net` 转出），而那个 crate
/// 是 native-only 依赖，wasm 下根本不在依赖树里。上层若直接持有它，就得为它写
/// `cfg(wasm_browser)` 分支——而「业务层不写 cfg」是本仓的硬约束。
///
/// 边界因此划在**本类型**而不是端口上：`WebTransportConfig` 是个两 target 都存在的不透明
/// 句柄，只有 native 宿主能用 [`with_store`](Self::with_store) 往里放东西。组合根
/// （`swarmdrop_core::start_node`）转发它即可，既不认识证书、也不写分支。
///
/// 上一版是反过来的——在这里镜像一份平台中立的 trait，再用适配器转回去。那要复制 4 个
/// 条目（trait、错误、内存实现、适配器），错误的 source chain 在转换中丢掉，而 wasm 侧
/// 拿到的字段**根本没人读**（那边的启用判据是「有没有 `WebTransport` API」）。
#[derive(Clone)]
pub struct WebTransportConfig {
    #[cfg(not(wasm_browser))]
    store: Option<Arc<dyn webtransport_p2p::CertificateStore>>,
}

impl WebTransportConfig {
    /// 只拨号，不监听 —— 因此没有服务端证书要持久化。**浏览器只有这一种形态。**
    pub fn client_only() -> Self {
        Self {
            #[cfg(not(wasm_browser))]
            store: None,
        }
    }

    /// 带服务端证书持久化。**监听方必须用它**：不持久化的话每次重启 certhash 都变，
    /// 对端记下的地址全部失效。
    ///
    /// 给了它，[`bind`](crate::Builder::bind) 还会自动补一条 WebTransport 监听地址
    /// （除非调用方已经显式给了一条）—— 「有证书可存」与「要监听」是同一个意图。
    #[cfg(not(wasm_browser))]
    pub fn with_store(store: Arc<dyn webtransport_p2p::CertificateStore>) -> Self {
        Self { store: Some(store) }
    }

    #[cfg(not(wasm_browser))]
    pub(crate) fn store(&self) -> Option<&Arc<dyn webtransport_p2p::CertificateStore>> {
        self.store.as_ref()
    }
}

impl std::fmt::Debug for WebTransportConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("WebTransportConfig");
        #[cfg(not(wasm_browser))]
        s.field("store", &self.store.as_ref().map(|_| "<injected>"));
        s.finish()
    }
}

/// 内部装配配置（Builder 收集、bind 时消费）。
#[derive(Clone)]
pub(crate) struct EndpointConfig {
    /// identify 的 protocol_version。默认是中立的内核值——业务协议契约
    /// （如 `/swarmdrop/2.0.0`）由上层经 `Builder::identify_protocol` 显式注入。
    pub identify_protocol: String,
    /// identify 的 agent_version（可含设备能力信息）。
    ///
    /// **运行时可变**（`Endpoint::set_agent_version`），不是只在构造期读一次的配置。
    /// 权威副本在 identify 的 `Behaviour` 里——新连接的 handler 从那里取值；
    /// 这里的字段只是内核自己的诊断镜像，两者由同一条 actor 命令一起更新。
    pub agent_version: String,
    pub ping_interval: Duration,
    pub ping_timeout: Duration,
    /// 空闲连接回收时限（keep_alive 白名单内的 peer 豁免）。
    pub idle_timeout: Duration,
    pub dht: Option<DhtConfig>,
    /// mDNS 局域网发现（wasm 下忽略——behaviour 字段编译期不存在）。
    pub mdns: bool,
    pub autonat: bool,
    /// AutoNAT v2 服务端：为其他节点执行公网可达性回拨探测。
    ///
    /// 仅公网引导节点应启用；普通客户端只需 [`Self::autonat`] 的客户端行为。
    pub autonat_server: bool,
    pub dcutr: bool,
    pub relay_client: bool,
    /// 中继服务端（LanHelper；wasm 下忽略——behaviour 字段编译期不存在）。
    pub relay_server: Option<RelayServerConfig>,
    /// webrtc-direct 的持久化证书（PEM）。**certhash 进分享地址——证书变则
    /// 分享出去的地址全部失效**，生产必须注入持久化证书（宿主身份存储）；
    /// `None` 时每次随机生成（仅测试/临时场景可接受）。native only。
    pub webrtc_cert_pem: Option<String>,
    /// WebRTC 打洞传输（`None` = 不启用）。双 target 均可用。
    pub webrtc_p2p: Option<WebRtcP2pConfig>,
    /// WebTransport 传输（`None` = 不启用）。
    ///
    /// **wasm 下这个字段被忽略**：浏览器侧只拨号、无服务端证书，启用判据是
    /// 「浏览器有没有 `WebTransport` API」。字段仍然存在，是为了让上层组合根用同一份
    /// 无分支的代码装配两个 target。
    pub webtransport: Option<WebTransportConfig>,
    /// 显式登记为本节点外部可达的地址。
    ///
    /// 公网 relay 通常监听 `0.0.0.0`，而 reservation 应答必须返回公网地址；
    /// 该字段让组合根在 bind 后立即把已知公网 TCP/QUIC/WS 地址注册给 Swarm。
    pub external_addrs: Vec<Addr>,
    /// 本节点的公网 IP（静态 1:1 NAT / 直接持有公网 IP 时给）。
    ///
    /// 给了就由内核持续维护「每条监听地址换上这个 IP」的那一份 external 地址，跟着监听
    /// 集合增删。判据见 [`Builder::external_ip`](crate::endpoint::builder::Builder::external_ip)。
    pub external_ip: Option<std::net::IpAddr>,
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
            autonat_server: false,
            dcutr: false,
            relay_client: true,
            relay_server: None,
            webrtc_cert_pem: None,
            webrtc_p2p: None,
            webtransport: None,
            external_addrs: Vec::new(),
            external_ip: None,
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
            .field("autonat_server", &self.autonat_server)
            .field("dcutr", &self.dcutr)
            .field("relay_client", &self.relay_client)
            .field("relay_server", &self.relay_server)
            // 绝不打印证书私钥材料，只标注是否已注入持久化证书
            .field(
                "webrtc_cert_pem",
                &self.webrtc_cert_pem.as_ref().map(|_| "<redacted>"),
            )
            .field("webrtc_p2p", &self.webrtc_p2p)
            // 委托给 `WebTransportConfig` 自己的 Debug —— 它已经打印了同一个 bit
            // （store 有没有），在这里重新推导一遍只会多一处会漂移的格式。
            .field("webtransport", &self.webtransport)
            .field("listen", &self.listen)
            .field("external_addrs", &self.external_addrs)
            .field("external_ip", &self.external_ip)
            .field("stream_limits", &self.stream_limits)
            .field("connect_timeout", &self.connect_timeout)
            .finish()
    }
}
