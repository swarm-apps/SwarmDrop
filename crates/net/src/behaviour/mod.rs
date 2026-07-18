//! Swarm behaviour 组装（无业务泛型——旧栈的 `CoreBehaviour<Req, Resp>`
//! 泛型随 request-response behaviour 一起消失，控制面走 stream 上的 RPC）。
//!
//! wasm 差异用 cfg 字段表达（学 iroh `Transports.ip`）：mdns / autonat /
//! dcutr / relay-server 在浏览器 target 下整个字段不存在。

pub(crate) mod keep_alive;

use libp2p::identity::Keypair;
use libp2p::swarm::NetworkBehaviour;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::{identify, kad, ping, relay};

use crate::config::EndpointConfig;

#[cfg(not(wasm_browser))]
use libp2p::{autonat, dcutr, mdns};

#[derive(NetworkBehaviour)]
pub(crate) struct Behaviour {
    /// RTT 测量 / 对端存活探测。注意 libp2p 0.52+ 起 ping 不保活连接
    /// （ping 流被 ignore_for_keep_alive 排除），保活由 keep_alive 按白名单承担。
    pub ping: ping::Behaviour,
    /// 逐 peer 保活白名单（已配对设备豁免空闲回收）。
    pub keep_alive: keep_alive::Behaviour,
    /// 节点信息交换（agent/protocol 版本、地址上报）。
    pub identify: identify::Behaviour,
    /// Kademlia DHT（跨网络发现 + 分享码/在线宣告 record）。`None` = 未启用。
    pub kad: Toggle<kad::Behaviour<kad::store::MemoryStore>>,
    /// 中继客户端（native 与浏览器都要：浏览器经 circuit 被动接收连接）。
    pub relay_client: Toggle<relay::client::Behaviour>,
    /// mDNS 局域网发现（浏览器无 UDP 多播，编译期不存在）。
    #[cfg(not(wasm_browser))]
    pub mdns: Toggle<mdns::tokio::Behaviour>,
    /// AutoNAT v2 客户端：让对端回拨确认外部可达性。
    #[cfg(not(wasm_browser))]
    pub autonat: Toggle<autonat::v2::client::Behaviour>,
    /// DCUtR 打洞协调（需要直连 socket，浏览器编译期不存在）。
    #[cfg(not(wasm_browser))]
    pub dcutr: Toggle<dcutr::Behaviour>,
    /// 中继服务端（LanHelper 模式）。类型在 wasm 也存在（relay feature），
    /// 无需 cfg——wasm 下配置恒 None。
    pub relay_server: Toggle<relay::Behaviour>,
    /// 应用字节流（Router / Endpoint::open 的底座）。
    pub stream: libp2p_stream::Behaviour,
}

impl Behaviour {
    pub(crate) fn new(
        keypair: &Keypair,
        relay_client: Option<relay::client::Behaviour>,
        config: &EndpointConfig,
    ) -> Self {
        let peer_id = keypair.public().to_peer_id();

        let ping = ping::Behaviour::new(
            ping::Config::new()
                .with_interval(config.ping_interval)
                .with_timeout(config.ping_timeout),
        );

        let identify = identify::Behaviour::new(
            identify::Config::new(config.identify_protocol.clone(), keypair.public())
                .with_agent_version(config.agent_version.clone())
                .with_push_listen_addr_updates(true)
                .with_cache_size(100),
        );

        let kad = Toggle::from(config.dht.as_ref().map(|dht| {
            let mut kad_config = kad::Config::default();
            kad_config
                .set_query_timeout(dht.query_timeout)
                .set_record_ttl(Some(dht.record_ttl))
                .set_replication_factor(dht.replication_factor)
                .set_publication_interval(Some(dht.publication_interval))
                .set_provider_record_ttl(Some(dht.record_ttl));
            let mut kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                kad_config,
            );
            // 默认由 AutoNAT 自动判定模式（确认公网可达后才切 Server）。
            // 已知可达的场景（测试、引导节点、LanHelper）强制 Server。
            if dht.server_mode {
                kad.set_mode(Some(kad::Mode::Server));
            }
            kad
        }));

        #[cfg(not(wasm_browser))]
        let mdns = Toggle::from(if config.mdns {
            Some(
                mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                    .expect("mDNS initialization failed"),
            )
        } else {
            None
        });

        #[cfg(not(wasm_browser))]
        let autonat = Toggle::from(config.autonat.then(autonat::v2::client::Behaviour::default));

        #[cfg(not(wasm_browser))]
        let dcutr = Toggle::from(config.dcutr.then(|| dcutr::Behaviour::new(peer_id)));

        // wasm 下 relay_server 配置被忽略（浏览器不可能当中继服务端）
        #[cfg(wasm_browser)]
        let relay_server = Toggle::from(None);
        #[cfg(not(wasm_browser))]
        let relay_server = Toggle::from(config.relay_server.as_ref().map(|s| {
            let mut server = relay::Behaviour::new(
                peer_id,
                relay::Config {
                    max_reservations: s.max_reservations,
                    max_reservations_per_peer: s.max_reservations_per_peer,
                    reservation_duration: s.reservation_duration,
                    max_circuits: s.max_circuits,
                    max_circuits_per_peer: s.max_circuits_per_peer,
                    max_circuit_duration: s.max_circuit_duration,
                    max_circuit_bytes: s.max_circuit_bytes,
                    ..Default::default()
                },
            );
            // ⚠️ master 行为变更（relay 0.22.0，PR 6154）：HOP 协议广告默认
            // `Status::Disable` 且随 external address 自动开关——LanHelper 是
            // 私网中继（无公网地址），auto 模式永远不会开 HOP，reservation
            // 请求会在 multistream 层被静默拒绝。配置了 relay_server 即意味着
            // 「我要当中继」，显式 Enable。
            server.set_status(Some(relay::Status::Enable));
            server
        }));

        Self {
            ping,
            keep_alive: keep_alive::Behaviour::default(),
            identify,
            kad,
            relay_client: Toggle::from(relay_client),
            #[cfg(not(wasm_browser))]
            mdns,
            #[cfg(not(wasm_browser))]
            autonat,
            #[cfg(not(wasm_browser))]
            dcutr,
            relay_server,
            stream: libp2p_stream::Behaviour::new(),
        }
    }
}
