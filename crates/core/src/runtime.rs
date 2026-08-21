//! Core runtime 入口。
//!
//! 装配新内核：`Endpoint`（builder + preset + DHT + 在线记录 lookup）→ 注册引导节点 →
//! 建 `NetManager`（pairing / devices / presence / infra）→ 建 `Router`（pairing /
//! transfer-ctrl / transfer-data 三协议）→ 返回订阅句柄供事件循环消费。

use std::sync::Arc;

use swarmdrop_net::{
    AddressLookup, Builder, DhtConfig, Endpoint, Events, InfraRoles, LookupBuilderFn,
    RelayServerConfig, Router, SecretKey, WebRtcP2pConfig, WebTransportConfig, presets,
};

use crate::device::{DeviceName, OsInfo};
use crate::error::{AppError, AppResult};
use crate::host::{DeviceConfig, EventBus, Notifier, PairedDeviceStore};
use crate::network::CandidateScope;
use crate::network::NetManager;
use crate::network::config::{
    NetworkRuntimeConfig, bootstrap_node_addrs, create_candidate_manager,
};
use crate::pairing::{PairingManager, PairingPorts, PairingService};
use crate::presence::OnlineRecordLookup;
use crate::protocol::{
    IDENTIFY_PROTOCOL, PAIRING, PAIRING_PROTOCOL, TRANSFER_CTRL, TRANSFER_CTRL_PROTOCOL,
    TRANSFER_DATA_PROTOCOL,
};
use crate::transfer::incoming::TransferCtrlService;
use crate::transfer::manager::TransferManager;
use crate::transfer::text_delivery::{TEXT_DELIVERY, TEXT_DELIVERY_PROTOCOL, TextDeliveryService};
use crate::transfer::wire::TransferDataHandler;
use swarmdrop_invite::InviteStore;

/// 已启动的 core 节点。
///
/// `events` 交给 [`run_event_loop`](crate::network::event_loop::run_event_loop) 消费；
/// `router` / `endpoint` 需与 `manager` 同生命周期（drop 或 `manager.shutdown()` 关停）。
pub struct StartedNode {
    pub endpoint: Endpoint,
    pub manager: NetManager<TransferManager>,
    pub router: Router,
    pub events: Events,
}

/// 节点端点形态——把 [`build_endpoint`] 里原先 Native-hardcoded 的策略收成可注入判别。
///
/// preset 是「立即 `apply` 的 setter 包」（见 [`swarmdrop_net::presets`]），无法作为值延迟
/// 存储，故用枚举判别而非「存 preset 值的 struct」。桌面/移动传 [`Native`](Self::Native)、
/// 浏览器壳传 [`Browser`](Self::Browser)——三端共享同一 `start_node` 组合根，仅端点形态与
/// 注入端口不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointProfile {
    /// 原生端（桌面/移动）：Native preset（TCP/QUIC 全网卡监听 + mDNS + AutoNAT + DCUtR +
    /// relay client）+ DHT 在线记录 lookup + 注册引导基础设施；LanHelper 时另开 relay server。
    Native,
    /// 浏览器端：Browser preset（不 listen 本地 socket、relay client）——不注册引导基础设施、
    /// 不开 relay server、不挂 DHT 在线 lookup（被动接收靠 `ensure_relay_reservation`）。
    Browser,
}

impl EndpointProfile {
    /// 是否注册引导/中继基础设施节点（浏览器端无内置引导，跳过整个注册循环）。
    fn registers_infra(self) -> bool {
        matches!(self, EndpointProfile::Native)
    }
}

/// 本机的**长期凭据**：身份密钥与各传输的证书材料。
///
/// 三项归在一起不是为了凑参数，而是它们共享同一条不变量：**全部由宿主持久化，且跨重启
/// 不变**。任何一项换了，此前分发出去的地址就集体失效——身份密钥换了对端认不出这是同一
/// 台设备，两份证书换了地址里的 `certhash` 随之改变，浏览器拨过去在 TLS 阶段被拒。
/// `crates/core/src/runtime.rs` 的三条 `*_survives_restart` / `*_reuses_certhash` 测试看守
/// 的就是这条。
pub struct NodeCredentials {
    /// Ed25519 身份密钥（身份存储里的存量为 protobuf 编码，与之完全兼容）。
    pub secret_key: SecretKey,
    /// webrtc-direct 的持久化证书 PEM。`None` = 每次随机生成（仅浏览器与临时场景）。
    pub webrtc_certificate_pem: Option<String>,
    /// WebTransport 配置，本质是它的**证书持久化端口**。`None` = 只拨号不监听；
    /// 宿主给一个带端口的配置才监听（`bind` 自动补监听地址）。
    /// core 因此既不认识证书、也不判断平台——浏览器构造不出带 store 的配置。
    pub webtransport: Option<WebTransportConfig>,
}

/// 宿主端口集 —— 三端各自实现（桌面文件 / 移动系统存储 / 浏览器 IndexedDB），
/// 由组合根一次注入。
///
/// 打包成 struct 的收益不在「参数少几个」，在于**位置换成了名字**：`None` 这样的实参
/// 在 12 个位置参数里要靠数数才知道是 notifier，而三个 host 各自还得在调用点写一行注释
/// 说明它是什么。加端口时也从「每个调用点静默移位」变成「每个调用点缺一个具名字段」。
pub struct HostPorts {
    /// 设备名等本机偏好。组合根从它读设备名填进 `OsInfo.name`。
    pub device_config: Arc<dyn DeviceConfig>,
    /// 已配对设备的**事实源**。组合根内部 load 一次建共享表，端口本身转交
    /// `PairingManager` 供 unpair 写回——两者同源，不是两个事实源。
    pub paired_device_store: Arc<dyn PairedDeviceStore>,
    /// core 事件出口（network / pairing / transfer 各域事件汇聚后交给宿主）。
    pub event_bus: Arc<dyn EventBus>,
    /// 系统通知。`None` = 该端没有这个概念（移动端与浏览器）。
    pub notifier: Option<Arc<dyn Notifier>>,
    /// 邀请注册表的落盘端口（native = `SqlInviteStore` / wasm = IndexedDB 实现）。
    /// 不需要持久化时传 `Arc::new(NoopInviteStore)`——退回「重启丢邀请」的旧语义。
    pub invite_store: Arc<dyn InviteStore>,
}

/// 启动 P2P 节点并装配 core 网络管理器与协议路由。
///
/// 设备名从 `device_config` 端口读出后塞入 `OsInfo.name`，经 identify 的 `agent_version`
/// 广播给对端，同时随 `OsInfo` 一路传到 `PairingManager`（配对请求与邀请串的 display）。
/// **本机 `OsInfo` 的装配点只此一处**——宿主传不进 `name`（`OsInfo::native()` 已去参）。
/// 这里装配的是**初值**：运行期改名走
/// [`device_name::rename_device`](crate::device_name::rename_device)，它同时更新
/// `PairingManager` 的本机 `OsInfo` 与 identify 的 `agent_version`，不重启节点。
///
/// 已配对设备的事实源是 [`HostPorts::paired_device_store`]，本函数内部 load 一次——调用方
/// **不再预先加载、也不再传快照**。（此前桌面还在快照之上叠了一层「存储为空则回退
/// 前端 IPC 传来的列表」的静默兜底，那份前端列表本身就是后端的镜像。）
pub async fn start_node<F>(
    credentials: NodeCredentials,
    os_info: OsInfo,
    network_config: NetworkRuntimeConfig,
    profile: EndpointProfile,
    ports: HostPorts,
    create_transfer: F,
) -> AppResult<StartedNode>
where
    F: FnOnce(Endpoint) -> TransferManager,
{
    let HostPorts {
        device_config,
        paired_device_store,
        event_bus,
        notifier,
        invite_store,
    } = ports;
    // 宿主传进来的 `os_info` **只承载平台探测部分**（hostname / os / platform / arch）：
    // native 走 `OsInfo::default()` 的 env 探测、wasm 走 `web_os_info()`——env 探测在 wasm
    // 恒 unknown，故必须由调用方注入。
    //
    // `name`（用户设的设备名）则由本函数从 `DeviceConfig` 端口填充：宿主无法注入它
    // （`OsInfo` 没有带 name 的构造入口），本机 `OsInfo` 于是只有这一个装配点。端口 load 不返回
    // 错误——读坏时降级成 `None`（回退 hostname）继续跑，不能让节点起不来。
    let mut os_info = os_info;
    os_info.name = device_config
        .load_device_name()
        .await
        .map(DeviceName::into_string);

    // LAN Helper 能力由 `network_config` 决定，在算 agent_version 前叠加，保持
    // `to_agent_version()` 契约。
    let os_info = if network_config.provide_lan_helper {
        os_info.with_capability(OsInfo::LAN_HELPER_CAPABILITY)
    } else {
        os_info
    };
    let agent_version = os_info.to_agent_version();

    let endpoint = build_endpoint(credentials, agent_version, &network_config, profile).await?;

    // 注册引导节点为 **DHT 种子**（bootstrap 依赖至少一个 kad server 进路由表）。
    // 浏览器端无内置引导，整循环跳过。
    //
    // **kad 角色无条件给，relay 角色就地过闸门。** 闸门查询放在这里而不是把 relay
    // 整个推迟给 `InfraSupervisor::tick`：推迟会让每个引导节点先以 `relay: false`
    // 注册一次、约一个 tick 后再以 kad+relay 注册第二次——首次可达性凭空晚一个 tick，
    // 注册工作还做两遍。
    //
    // 判据必须与 `exclusion_for` 同一条（候选**持有公网地址** ⇒ 受开关约束），
    // 否则同一个节点在启动路径和收敛环里得到两种结论：私网引导节点被全局开关一刀切
    // 拦掉，而 tick 又会把它加回来，每次冷启动都白拨一轮。
    //
    // 反过来也不能把这整段删掉改走候选表：`wants_reservation` 要求 relay 角色，
    // `public_reachability=false` 时公网候选一次 `add_infrastructure_peer` 都不会发，
    // kad 路由表拿不到任何公网种子 → `dht.bootstrap()` 与在线记录发布全塌。
    // 两个开关是正交的（见 `NetworkRuntimeConfig` 上的注释），不能让一个吃掉另一个。
    if profile.registers_infra() {
        for peer in bootstrap_node_addrs(&network_config) {
            let relay = network_config.public_reachability
                || CandidateScope::infer(&peer.addrs) == CandidateScope::Lan;
            if let Err(e) = endpoint
                .add_infrastructure_peer(
                    peer,
                    InfraRoles {
                        kad_server: true,
                        relay,
                    },
                )
                .await
            {
                tracing::warn!("注册引导节点失败: {e}");
            }
        }
    }

    let events = endpoint
        .subscribe()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    let transfer = create_transfer(endpoint.clone());
    transfer.recover_interrupted_text_deliveries().await?;
    let candidate_manager = create_candidate_manager(&network_config);
    // 快照与端口一并交给 NetManager：构造是同步的，共享 DashMap 的初值只能由这里先
    // load 出来，端口本身则转交 PairingManager 供 unpair 写回。两者同源，不是两个事实源。
    let paired_devices = paired_device_store.load_paired_devices().await?;
    let manager = NetManager::new(
        endpoint.clone(),
        os_info,
        paired_devices,
        transfer,
        network_config,
        candidate_manager,
        PairingPorts {
            event_bus,
            notifier: notifier.clone(),
            invite_store,
            paired_store: paired_device_store,
        },
    );

    // 把落盘的邀请读回内存表。必须在对外服务之前完成：注册表是一次性消费的权威判定点，
    // 没 load 就等于「重启后所有已发出的邀请都不认识」，对方点开链接只会拿到「邀请无效」。
    manager.pairing().load_invites().await;

    // Router：三协议入站路由。pairing 与 transfer 控制面是 typed RPC，数据面是裸流。
    let router = build_router(
        &endpoint,
        manager.pairing_arc(),
        manager.transfer_arc(),
        notifier,
    );

    Ok(StartedNode {
        endpoint,
        manager,
        router,
        events,
    })
}

/// 装配入站 [`Router`] 并 spawn。
///
/// pairing 与 transfer 控制面是 typed RPC（[`PAIRING`] / [`TRANSFER_CTRL`]），数据面
/// （[`TRANSFER_DATA_PROTOCOL`]）是裸流。
/// runtime 与 e2e 测试共用同一套装配，避免协议注册漂移。返回的 `Router` 需与 `endpoint`
/// 同生命周期。
pub fn build_router(
    endpoint: &Endpoint,
    pairing: Arc<PairingManager>,
    transfer: Arc<TransferManager>,
    notifier: Option<Arc<dyn Notifier>>,
) -> Router {
    let text_delivery = Arc::new(TextDeliveryService::new(
        Arc::downgrade(&transfer),
        pairing.clone(),
        endpoint.clone(),
        notifier.clone(),
    ));
    transfer.set_text_delivery_service(text_delivery.clone());
    Router::builder(endpoint.clone())
        .accept(
            PAIRING_PROTOCOL,
            PAIRING.handler(PairingService(pairing.clone())),
        )
        .accept(
            TRANSFER_CTRL_PROTOCOL,
            TRANSFER_CTRL.handler(TransferCtrlService::new(
                transfer.clone(),
                pairing,
                endpoint.clone(),
                notifier,
            )),
        )
        .accept(
            TEXT_DELIVERY_PROTOCOL,
            TEXT_DELIVERY.handler(text_delivery.as_ref().clone()),
        )
        // 数据面**只挂一个协议名**。曾同时挂过 `/2` 与 `/3`（两版只差「发送端发不发窗口
        // 帧」，接收端的读循环对两者都成立），随 bao chunk group 变更一并摘除。
        //
        // **这是有意识地放弃向后兼容**——SwarmHive 上 v0.12.1 至 v0.14.0 都已发布，摘除
        // 意味着与它们双向都传不了。理由不是「没有存量」，是当时的用户基数小到不值得
        // 为兼容付代价；判据与完整代价写在 `protocol.rs` 的 `TRANSFER_DATA_PROTOCOL`。
        //
        // 但留着注册更糟：验签树形状变了之后，旧名接进来的连接会在第一个块就验签失败、
        // 断流、Interrupted 无限重试——那等于把「版本不匹配」伪装成「传输老是断」。
        .accept(TRANSFER_DATA_PROTOCOL, TransferDataHandler::new(transfer))
        .spawn()
}

/// WebRTC 打洞用的 STUN 服务器（政策位，换供应商只改这里）。
///
/// 多列几家是因为**没有 STUN 就只有 host candidate，跨 NAT 必然打不通**——这不是可选
/// 优化。国内可达的排前面，Google 兜底：本项目的 bootstrap 在国内，只挂 Google 等于
/// 对主力用户群关掉打洞能力。
///
/// ⚠️ 严格说这是部署配置，按既有约定（net-kernel.md「公共基础设施地址由 Host 配置」）
/// 该由各 host 注入。当前只有浏览器一个消费方，先收在这里；等需要按端差异化时再上提。
const STUN_SERVERS: &[&str] = &[
    "stun:stun.qq.com:3478",
    "stun:stun.miwifi.com:3478",
    "stun:stun.l.google.com:19302",
];

/// 按 [`EndpointProfile`] + [`NetworkRuntimeConfig`] 装配并 bind 一个 Endpoint。
///
/// preset / DHT 在线记录 lookup 由 profile 决定（Native 挂 `OnlineRecordLookup`、Browser
/// 不挂）；LanHelper 模式 = relay server 开 + DHT server_mode（本机已知可达），否则 DHT
/// server_mode 由 AutoNAT 判定。在线记录 lookup 用 [`LookupBuilderFn`] 延迟构造（构造依赖
/// 已 bind 的 Endpoint）。identify 协议 / agent_version / DHT server_mode 三端一致。
async fn build_endpoint(
    credentials: NodeCredentials,
    agent_version: String,
    config: &NetworkRuntimeConfig,
    profile: EndpointProfile,
) -> AppResult<Endpoint> {
    let dht_config = DhtConfig {
        server_mode: config.provide_lan_helper,
        ..DhtConfig::default()
    };
    let mut builder = Endpoint::builder()
        .secret_key(credentials.secret_key)
        .identify_protocol(IDENTIFY_PROTOCOL)
        .agent_version(agent_version)
        .dht(dht_config);

    if let Some(pem) = credentials.webrtc_certificate_pem {
        builder = builder.webrtc_certificate(pem);
    }

    builder = match profile {
        EndpointProfile::Native => builder
            .preset(presets::Native)
            .address_lookup(LookupBuilderFn(|ep: &Endpoint| {
                Ok(Box::new(OnlineRecordLookup::new(ep.clone())) as Box<dyn AddressLookup>)
            })),
        EndpointProfile::Browser => builder.preset(presets::Browser),
    };

    builder = with_transports(builder, credentials.webtransport);

    // relay server 仅 Native + LanHelper（Browser 是纯 relay client，永不当 server）。
    // 这里判据是「端点形态是否 Native」，与 `registers_infra()`（是否注册引导设施）语义无关，
    // 只是当前恰好都对 Native 为真——直接 match，避免借用不相干的谓词把两个决策耦死。
    if matches!(profile, EndpointProfile::Native) && config.provide_lan_helper {
        builder = builder.relay_server(RelayServerConfig::default());
    }
    builder
        .bind()
        .await
        .map_err(|e| AppError::Network(e.to_string()))
}

/// 传输侧的装配。
///
/// **两条路径共用它**：真 bind（[`build_endpoint`]）与「不 bind，只问会装配出什么」
/// （[`planned_transports`]）。分出来是为了让后者不可能给出与前者不同的答案——那个答案
/// 是「用户粘的这条引导节点地址本端拨不拨得动」的唯一判据，多报一种会让用户配下一条
/// 永远连不上的地址且没有任何提示。
fn with_transports(builder: Builder, webtransport: Option<WebTransportConfig>) -> Builder {
    // WebRTC 打洞：**两端都开**。
    //
    // 浏览器非开不可——它没有 DCUtR（那需要直连 socket，wasm 编译期就不存在），经 relay
    // 换信令是它把中转升级成直连的唯一途径。而原生端也必须开，因为**打洞要两端都支持**：
    // 只有浏览器开着，`web ↔ NAT 后的桌面/手机` 这一格照样是全程中转，而那恰恰是自研这个
    // 传输最想拿下的场景。
    //
    // 对原生端不是冗余：dcutr 走 TCP/QUIC 直连，ICE 走 UDP + STUN 候选，两者覆盖的 NAT
    // 类型不同，互为补充。
    let builder = builder.webrtc_p2p(WebRtcP2pConfig {
        stun_servers: STUN_SERVERS.iter().map(|s| s.to_string()).collect(),
        ..WebRtcP2pConfig::default()
    });

    // WebTransport：**拨号能力无条件开**，否则 `/quic-v1/webtransport` 地址会以
    // `MultiaddrNotSupported` 直接失败，本端连 bootstrap 的 4004 都拨不了。
    //
    // 要不要**监听**由宿主决定 —— 它给了带证书端口的配置就监听，`bind` 会自动补那条
    // 监听地址（见 `ensure_webtransport_listen`）。core 因此既不认识证书、也不判断平台：
    // 浏览器构造不出带 store 的配置（`with_store` 是 native-only），天然只拨号。
    //
    // 因此 `webtransport = None` 与 `Some(带证书的配置)` 在**可拨传输**上是同一个答案，
    // [`planned_transports`] 不必知道宿主给没给证书端口。
    builder.webtransport(webtransport.unwrap_or_else(WebTransportConfig::client_only))
}

/// 该 profile 下会装配出哪些可拨传输，**不 bind**。
///
/// 用途是「没有节点在跑时也要校验引导节点地址」：那条校验的第三条规则问的是本端点装配
/// 了哪些传输（见 [`crate::infra::validate_shape`]），而为了回答它去 bind 一次，等于让
/// 一条只读本机记录的命令连引导节点、做 NAT 探测。命令行宿主的 `bootstrap add` 是第一个
/// 消费者。
///
/// 答案与真 bind 出来的端点逐字相同，由 [`with_transports`] 的共用与
/// `planned_transports_match_a_real_endpoint` 那条护栏测试共同保证。
pub fn planned_transports(profile: EndpointProfile) -> Vec<swarmdrop_net::TransportKind> {
    let builder = match profile {
        EndpointProfile::Native => Endpoint::builder().preset(presets::Native),
        EndpointProfile::Browser => Endpoint::builder().preset(presets::Browser),
    };
    with_transports(builder, None).supported_transports()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每次都是**新身份**：这些用例验证的是装配路径，不是身份复用；要验证跨重启复用的
    /// 那两条各自显式传同一份证书 / 同一个 store。
    fn credentials(webtransport: Option<WebTransportConfig>) -> NodeCredentials {
        NodeCredentials {
            secret_key: SecretKey::generate(),
            webrtc_certificate_pem: None,
            webtransport,
        }
    }

    /// 回归锚点：Native profile 装配（TCP/QUIC listen + DHT lookup）能 bind——等价泛化前
    /// 的硬编码路径。Browser profile（空 listen + relay client、无 lookup/infra）也能 bind，
    /// 结构上验证「同一 `build_endpoint` 吃两种形态」。
    #[tokio::test]
    async fn both_profiles_bind() {
        let config = NetworkRuntimeConfig::default();

        let native = build_endpoint(
            credentials(None),
            "swarmdrop/test".to_string(),
            &config,
            EndpointProfile::Native,
        )
        .await
        .expect("native profile bind");
        native.close().await;

        let browser = build_endpoint(
            credentials(None),
            "swarmdrop/test".to_string(),
            &config,
            EndpointProfile::Browser,
        )
        .await
        .expect("browser profile bind");
        browser.close().await;
    }

    /// **不 bind 算出来的可拨传输，必须与真 bind 出来的逐字相同。**
    ///
    /// 这条看守的是一处静默失效：[`planned_transports`] 多报一种，用户就能在没有节点在跑
    /// 时配下一条本端永远拨不动的引导节点地址，而校验会放行、UI 也说得出「本端支持它」；
    /// 少报一种则当场拒掉一条合法地址。两种都不会有任何错误信息。
    #[tokio::test]
    async fn planned_transports_match_a_real_endpoint() {
        let native = build_endpoint(
            credentials(None),
            "swarmdrop/test".to_string(),
            &NetworkRuntimeConfig::default(),
            EndpointProfile::Native,
        )
        .await
        .expect("native profile bind");

        assert_eq!(
            planned_transports(EndpointProfile::Native),
            native.supported_transports(),
            "不 bind 的传输清单与真端点不一致"
        );
        native.close().await;
    }

    /// 轮询直到 `native` 的 dialable 地址集出现 webrtc-direct 地址，返回其 certhash 段。
    async fn wait_for_webrtc_certhash(native: &Endpoint) -> String {
        let mut addrs = native.watch_addrs();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(hash) = addrs.get().dialable().iter().find_map(|addr| {
                    let s = addr.to_string();
                    s.contains("/webrtc-direct/certhash/")
                        .then(|| s.split("/certhash/").nth(1).map(str::to_owned))
                        .flatten()
                }) {
                    break hash;
                }
                addrs.updated().await.expect("address watch closed");
            }
        })
        .await
        .expect("native profile should publish a webrtc-direct address")
    }

    /// 轮询直到出现一条 WebTransport 监听地址，返回它的 certhash 段（第一个）。
    async fn wait_for_webtransport_certhash(native: &Endpoint) -> String {
        let mut addrs = native.watch_addrs();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(hash) = addrs.get().dialable().iter().find_map(|addr| {
                    let s = addr.to_string();
                    s.contains("/webtransport/certhash/")
                        .then(|| s.split("/webtransport/certhash/").nth(1))
                        .flatten()
                        // 地址上有**两个** certhash（current + next，spec 要求同时通告），
                        // 只取第一个。
                        .and_then(|rest| rest.split("/certhash/").next())
                        .map(str::to_owned)
                }) {
                    break hash;
                }
                addrs.updated().await.expect("address watch closed");
            }
        })
        .await
        .expect("注入证书端口的 Native profile 应发布 WebTransport 地址")
    }

    /// 注入证书端口后，Native profile 必须真的**监听** WebTransport。
    ///
    /// 没有这条，「桌面端接入」就只有装配代码而没有任何证据。
    #[tokio::test]
    async fn native_with_cert_store_listens_on_webtransport() {
        let native = build_endpoint(
            credentials(Some(WebTransportConfig::with_store(Arc::new(
                swarmdrop_net::WebTransportMemoryCertificateStore::default(),
            )))),
            "swarmdrop/test".to_string(),
            &NetworkRuntimeConfig::default(),
            EndpointProfile::Native,
        )
        .await
        .expect("native profile bind");

        wait_for_webtransport_certhash(&native).await;
        native.close().await;
    }

    /// 反面：不给证书端口就**不监听**（只拨号）。
    ///
    /// 这条挡的是「把监听判据从『有没有端口』悄悄改成『是不是 Native』」——那样移动端会
    /// 凭空多出一个监听端口，且 certhash 每次启动都变。
    #[tokio::test]
    async fn native_without_cert_store_does_not_listen_on_webtransport() {
        let native = build_endpoint(
            credentials(None),
            "swarmdrop/test".to_string(),
            &NetworkRuntimeConfig::default(),
            EndpointProfile::Native,
        )
        .await
        .expect("native profile bind");

        // 先等一条别的监听地址出现，证明监听确实起来了 —— 否则这条断言在「什么都还没
        // 监听」的瞬间也会通过，成为一条永远为真的假绿测试。
        wait_for_webrtc_certhash(&native).await;

        assert!(
            !native
                .watch_addrs()
                .get()
                .dialable()
                .iter()
                .any(|a| a.to_string().contains("/webtransport")),
            "没有证书端口时不该监听 WebTransport"
        );
        native.close().await;
    }

    /// **持久化的证明**：同一个 store 两次装配，certhash 必须一致。
    ///
    /// 它不一致的后果是静默的 —— 每次重启后对端记下的 WebTransport 地址全部拨不通，
    /// 而本机日志里什么都看不到（对端只是换用了别的传输，或者干脆连不上）。
    /// 与 `build_endpoint_reuses_certhash_for_same_persisted_pem` 是同一条不变量在
    /// 另一个传输上的版本。
    #[tokio::test]
    async fn webtransport_certhash_survives_restart_with_same_store() {
        let store = Arc::new(swarmdrop_net::WebTransportMemoryCertificateStore::default());

        let mut hashes = Vec::new();
        for _ in 0..2 {
            let native = build_endpoint(
                credentials(Some(WebTransportConfig::with_store(store.clone()))),
                "swarmdrop/test".to_string(),
                &NetworkRuntimeConfig::default(),
                EndpointProfile::Native,
            )
            .await
            .expect("native profile bind");

            hashes.push(wait_for_webtransport_certhash(&native).await);
            native.close().await;
        }

        assert_eq!(
            hashes[0], hashes[1],
            "同一份持久化证书两次装配的 certhash 必须一致，否则重启即失效"
        );
    }

    #[tokio::test]
    async fn native_profile_publishes_webrtc_direct_address() {
        let native = build_endpoint(
            credentials(None),
            "swarmdrop/test".to_string(),
            &NetworkRuntimeConfig::default(),
            EndpointProfile::Native,
        )
        .await
        .expect("native profile bind");

        wait_for_webrtc_certhash(&native).await;
        native.close().await;
    }

    /// 本 PR 修的核心不变量：`identity::load_or_create_webrtc_certificate` 持久化的 PEM
    /// 经 `build_endpoint` 两次装配后 certhash 必须一致，否则已分发的邀请地址会随每次
    /// 重启失效。`crates/net/tests/webrtc.rs` 已在 `Endpoint::builder()` 这一层验证过
    /// 同一 PEM 两次 bind 的 certhash 相等；这里补的是本 PR 新增的 core 集成点——
    /// `build_endpoint` 的 `webrtc_certificate_pem` 装配路径本身不会漂移。
    #[tokio::test]
    async fn build_endpoint_reuses_certhash_for_same_persisted_pem() {
        let pem = swarmdrop_net::generate_webrtc_certificate_pem().expect("generate cert");

        let mut hashes = Vec::new();
        for _ in 0..2 {
            let native = build_endpoint(
                NodeCredentials {
                    webrtc_certificate_pem: Some(pem.clone()),
                    ..credentials(None)
                },
                "swarmdrop/test".to_string(),
                &NetworkRuntimeConfig::default(),
                EndpointProfile::Native,
            )
            .await
            .expect("native profile bind");

            hashes.push(wait_for_webrtc_certhash(&native).await);
            native.close().await;
        }

        assert_eq!(
            hashes[0], hashes[1],
            "同一持久化证书经 build_endpoint 两次装配，certhash 必须一致"
        );
    }
}
