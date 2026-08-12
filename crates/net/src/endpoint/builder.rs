//! [`Builder`]：Endpoint 装配（对应旧栈 `start::<Req, Resp>`，泛型整体消失）。

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use swarmdrop_net_base::{Addr, NatStatus, SecretKey};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::presets::Preset;
use super::{AddrsInfo, Endpoint, Inner};
use crate::actor::{Actor, ActorMessage, WatchSenders};
use crate::config::{DhtConfig, EndpointConfig, RelayServerConfig, WebRtcP2pConfig};
use crate::dht::Dht;
use crate::lookup::AddressLookupBuilder;
use crate::stream::{StreamLimits, StreamRegistry};
use crate::transport::{BuildSwarmError, build_swarm};

const COMMAND_CHANNEL_SIZE: usize = 32;

/// `bind()` 的失败。
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error(transparent)]
    BuildSwarm(#[from] BuildSwarmError),
    #[error("listen on {addr} failed: {reason}")]
    Listen { addr: Addr, reason: String },
    #[error("address lookup construction failed: {0}")]
    Lookup(#[from] crate::lookup::LookupError),
}

/// Endpoint 配置 Builder。
///
/// `preset()` 先应用、后续 setter 可覆盖（学 iroh 的 Preset 语义）。
#[derive(Default)]
pub struct Builder {
    secret: Option<SecretKey>,
    config: EndpointConfig,
    lookups: Vec<Box<dyn AddressLookupBuilder>>,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder")
            .field("config", &self.config)
            .field("lookups", &self.lookups.len())
            .finish_non_exhaustive()
    }
}

impl Builder {
    /// 节点密钥。**不设则每次随机生成新身份**——持久化身份必须显式传入。
    pub fn secret_key(mut self, secret: SecretKey) -> Self {
        self.secret = Some(secret);
        self
    }

    /// 应用配置包（可叠加，后应用的覆盖先前值）。
    pub fn preset(self, preset: impl Preset) -> Self {
        preset.apply(self)
    }

    /// identify 的 protocol_version（协议兼容性检查用，如 `/swarmdrop/2.0.0`）。
    pub fn identify_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.config.identify_protocol = protocol.into();
        self
    }

    /// identify 的 agent_version（可携带设备能力信息）。
    ///
    /// 这里设的只是初值：运行期改用 [`Endpoint::set_agent_version`]，它会即时向
    /// 已连接对端 push，无需重建节点。
    pub fn agent_version(mut self, agent: impl Into<String>) -> Self {
        self.config.agent_version = agent.into();
        self
    }

    /// 监听地址（wasm 下必须为空：浏览器不能 listen 本地 socket）。
    pub fn listen(mut self, addrs: Vec<Addr>) -> Self {
        self.config.listen = addrs;
        self
    }

    /// 声明启动时已知的外部可达地址（等价于 bind 后立刻
    /// [`Endpoint::set_external_addrs`] 一次，但赶得上启动早期的 reservation 应答）。
    ///
    /// 典型场景是公网 relay 的 TCP/QUIC 地址。WebRTC Direct 的 certhash 可用
    /// [`crate::webrtc_direct_addr_from_pem`] 从同一持久化证书预先派生；地址会随证书轮换
    /// 变化的传输（WebTransport）则要在运行期用 [`Endpoint::set_external_addrs`] 持续声明
    /// ——那条**必须**跟着轮换更新，理由见该方法的文档。
    pub fn external_addrs(mut self, addrs: Vec<Addr>) -> Self {
        self.config.external_addrs = addrs;
        self
    }

    /// 启用 Kademlia DHT。
    pub fn dht(mut self, config: DhtConfig) -> Self {
        self.config.dht = Some(config);
        self
    }

    /// mDNS 局域网发现（wasm 下无效——behaviour 编译期不存在）。
    pub fn mdns(mut self, enabled: bool) -> Self {
        self.config.mdns = enabled;
        self
    }

    /// AutoNAT v2 外部可达性探测。
    pub fn autonat(mut self, enabled: bool) -> Self {
        self.config.autonat = enabled;
        self
    }

    /// AutoNAT v2 服务端（仅公网 bootstrap / relay 节点需要）。
    pub fn autonat_server(mut self, enabled: bool) -> Self {
        self.config.autonat_server = enabled;
        self
    }

    /// DCUtR 打洞。
    pub fn dcutr(mut self, enabled: bool) -> Self {
        self.config.dcutr = enabled;
        self
    }

    /// 中继客户端（浏览器被动接收连接依赖它，默认开）。
    pub fn relay_client(mut self, enabled: bool) -> Self {
        self.config.relay_client = enabled;
        self
    }

    /// 中继服务端（LanHelper 模式；wasm 下忽略）。
    pub fn relay_server(mut self, config: RelayServerConfig) -> Self {
        self.config.relay_server = Some(config);
        self
    }

    /// webrtc-direct 的持久化证书（PEM）。**certhash 进分享地址**——生产必须
    /// 注入持久化证书，否则重启后分享出去的地址全部失效。native only。
    pub fn webrtc_certificate(mut self, pem: impl Into<String>) -> Self {
        self.config.webrtc_cert_pem = Some(pem.into());
        self
    }

    /// 启用 WebTransport（native listener + dialer）。
    ///
    /// 「启用」与「服务端证书持久化」是两件事，由 [`WebTransportConfig`] 区分：
    /// 纯拨号方用 `client_only()`，监听方用 `with_store(..)`。后者不像 webrtc-direct
    /// 那样给一份 PEM 就完事 —— WebTransport 的证书 spec 强制 ≤ 14 天有效期，本机会自行
    /// 轮换并**回写**，宿主必须提供可写的存储。
    ///
    /// **native**：不调用它 = 不启用，`/quic-v1/webtransport` 地址会以
    /// `MultiaddrNotSupported` 快速失败，而不是挂着等超时。
    ///
    /// **浏览器：调用与否都一样。** 那边的启用判据是「有没有 `WebTransport` API」，这个
    /// 配置整个被忽略（它也没有 store 可放）。方法在两个 target 下都存在，是为了让上层
    /// 组合根用同一份无分支的代码装配三端。
    pub fn webtransport(mut self, config: crate::config::WebTransportConfig) -> Self {
        self.config.webtransport = Some(config);
        self
    }

    /// 启用 WebRTC 打洞传输（内核层面默认关；core 的组合根对三端一律开启）。
    ///
    /// 与 webrtc-direct 正交：后者要求目标地址已可达，前者让双方都不可达的节点
    /// （浏览器、NAT 后的原生端）经 relay 换信令后打洞。开启后本机既能拨
    /// `<relay>/p2p-circuit/webrtc/p2p/<target>`，也会为 circuit reservation
    /// 额外监听 `<relay>/p2p-circuit/webrtc` 以便被拨。
    pub fn webrtc_p2p(mut self, config: WebRtcP2pConfig) -> Self {
        self.config.webrtc_p2p = Some(config);
        self
    }

    /// 注册 pull 型地址解析源（`connect(NodeId)` 无候选地址时并发查询）。
    ///
    /// 构造依赖 Endpoint 的 lookup（如基于 DHT 的在线宣告）用
    /// [`LookupBuilderFn`](crate::lookup::LookupBuilderFn) 延迟构造。
    pub fn address_lookup(mut self, lookup: impl AddressLookupBuilder) -> Self {
        self.lookups.push(Box::new(lookup));
        self
    }

    /// 活跃流配额。
    pub fn stream_limits(mut self, limits: StreamLimits) -> Self {
        self.config.stream_limits = limits;
        self
    }

    /// 空闲连接回收时限（keep_alive 白名单豁免）。
    pub fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.config.idle_timeout = timeout;
        self
    }

    /// `connect()` 的整体超时。
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }

    /// 装配并启动内核：建 Swarm → 起监听 → spawn 中枢 actor → 回填 lookup。
    pub async fn bind(self) -> Result<Endpoint, BindError> {
        let secret = self.secret.unwrap_or_else(SecretKey::generate);
        let node_id = secret.node_id();
        let mut config = self.config;
        ensure_webtransport_listen(&mut config);

        let mut swarm = build_swarm(secret.as_keypair().clone(), &config)?;

        // 公网 relay 的 reservation 响应依赖 Swarm 已知 external address。
        // 在 actor 起前登记，避免启动早期的 reservation 漏带可拨地址。
        for addr in &config.external_addrs {
            swarm.add_external_address(addr.as_multiaddr().clone());
        }

        // 开流快路径句柄（Control 可 Clone，注册入站协议由 Router 在 spawn 时进行）
        let control = swarm.behaviour().stream.new_control();

        // 起监听（wasm 下 listen 为空，此循环不执行）
        for addr in &config.listen {
            swarm
                .listen_on(addr.as_multiaddr().clone())
                .map_err(|e| BindError::Listen {
                    addr: addr.clone(),
                    // `{e:?}` 而非 `to_string()`：`TransportError` 的 Display 在 `Other`
                    // 分支上写的是**空串**（libp2p `core/src/transport.rs`），而传输层的
                    // 真实失败（端口占用、地址不可绑）全落在那个分支——用 Display 的话
                    // bind 会以一句「listen on … failed: 」结束，什么都没说。
                    reason: format!("{e:?}"),
                })?;
        }

        // watch：actor 是唯一写者，Endpoint 持读端
        // `Swarm::add_external_address` 不会保证回发 ExternalAddrConfirmed；
        // 显式配置的公网地址由组合根负责正确性，故在状态视图中同步作为初值。
        // 经同一个规范化函数，与 actor 的 `resync_external` 算出的形状一致——否则
        // 配置里若有重复地址，第一次重算就会判为「变了」而白广播一轮。
        let (addrs_tx, addrs_rx) = watch::channel(AddrsInfo {
            listen: Vec::new(),
            external: crate::addrset::dedup_preserving_order(&config.external_addrs),
        });
        let (nat_tx, nat_rx) = watch::channel(NatStatus::default());
        let (conns_tx, conns_rx) = watch::channel(BTreeMap::new());
        let (relays_tx, relays_rx) = watch::channel(BTreeMap::new());

        let (actor_tx, actor_rx) = mpsc::channel::<ActorMessage>(COMMAND_CHANNEL_SIZE);
        let dht_enabled = config.dht.is_some();
        let actor = Actor::new(
            swarm,
            actor_rx,
            WatchSenders {
                addrs: addrs_tx,
                nat: nat_tx,
                conns: conns_tx,
                relays: relays_tx,
            },
            actor_tx.clone(),
            config.clone(),
            node_id,
        );
        let actor_handle = n0_future::task::spawn(actor.run());

        // builder 是 endpoint 的子模块，可直接构造私有字段的 `Inner`（免 InnerParts 搬运仪式）
        let endpoint = Endpoint::from_inner(Inner {
            node_id,
            actor_tx: actor_tx.clone(),
            control,
            registry: StreamRegistry::new(config.stream_limits),
            watch_addrs: addrs_rx,
            watch_nat: nat_rx,
            watch_conns: conns_rx,
            watch_relays: relays_rx,
            dht: dht_enabled.then(|| Dht::new(actor_tx.clone())),
            supported_transports: crate::transport::supported_transports(&config).into(),
            connect_timeout: config.connect_timeout,
            next_connect_request_id: std::sync::atomic::AtomicU64::new(1),
            closed: CancellationToken::new(),
            actor_handle: Mutex::new(Some(actor_handle)),
        });

        // lookup 回填：构造依赖已建好的 Endpoint（鸡生蛋），构造完注入 actor
        if !self.lookups.is_empty() {
            let mut lookups = Vec::with_capacity(self.lookups.len());
            for builder in self.lookups {
                lookups.push(builder.into_address_lookup(&endpoint)?);
            }
            let _ = actor_tx.send(ActorMessage::SetLookups(lookups)).await;
        }

        Ok(endpoint)
    }
}

/// 注入了证书端口却没给监听地址时，自动补一条 WebTransport 监听。
///
/// 「有服务端证书可存」与「要监听 WebTransport」是同一个意图 —— 拨号方不需要证书
/// （它只验对端的）。让调用方两处各表达一次，就会出现「给了 store 却没监听」这种
/// 半配置状态：证书照常生成、轮换、落盘，唯独没人能连进来，而且**没有任何报错**。
///
/// 端口给 `0` 由系统分配，certhash 在 transport 层补进最终监听地址。已经显式给了
/// WebTransport 监听地址的调用方（bootstrap 要固定 4004）不受影响。
///
/// wasm 下 `store` 字段根本不存在（`with_store` 是 native-only），因此浏览器永远走不到
/// 这里 —— 那正对：它起不了任何监听，凭空多一条会让 `bind` 直接失败。
#[cfg(not(wasm_browser))]
fn ensure_webtransport_listen(config: &mut EndpointConfig) {
    let wants_listen = config
        .webtransport
        .as_ref()
        .is_some_and(|wt| wt.store().is_some());
    if !wants_listen {
        return;
    }
    let already = config
        .listen
        .iter()
        .any(|a| a.transport() == Some(swarmdrop_net_base::TransportKind::Webtransport));
    if already {
        return;
    }
    config.listen.push(
        "/ip4/0.0.0.0/udp/0/quic-v1/webtransport"
            .parse()
            .expect("valid multiaddr"),
    );
}

/// 浏览器起不了监听，这里无事可做。
#[cfg(wasm_browser)]
fn ensure_webtransport_listen(_config: &mut EndpointConfig) {}
