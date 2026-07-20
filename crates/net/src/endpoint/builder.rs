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
use crate::config::{DhtConfig, EndpointConfig, RelayServerConfig};
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
    pub fn agent_version(mut self, agent: impl Into<String>) -> Self {
        self.config.agent_version = agent.into();
        self
    }

    /// 监听地址（wasm 下必须为空：浏览器不能 listen 本地 socket）。
    pub fn listen(mut self, addrs: Vec<Addr>) -> Self {
        self.config.listen = addrs;
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
        let config = self.config;

        let mut swarm = build_swarm(secret.as_keypair().clone(), &config).await?;

        // 开流快路径句柄（Control 可 Clone，注册入站协议由 Router 在 spawn 时进行）
        let control = swarm.behaviour().stream.new_control();

        // 起监听（wasm 下 listen 为空，此循环不执行）
        for addr in &config.listen {
            swarm
                .listen_on(addr.as_multiaddr().clone())
                .map_err(|e| BindError::Listen {
                    addr: addr.clone(),
                    reason: e.to_string(),
                })?;
        }

        // watch：actor 是唯一写者，Endpoint 持读端
        let (addrs_tx, addrs_rx) = watch::channel(AddrsInfo::default());
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
            connect_timeout: config.connect_timeout,
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
