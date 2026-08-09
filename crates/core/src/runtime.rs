//! Core runtime 入口。
//!
//! 装配新内核：`Endpoint`（builder + preset + DHT + 在线记录 lookup）→ 注册引导节点 →
//! 建 `NetManager`（pairing / devices / presence / infra）→ 建 `Router`（pairing /
//! transfer-ctrl / transfer-data 三协议）→ 返回订阅句柄供事件循环消费。

use std::sync::Arc;

use swarmdrop_net::{
    AddressLookup, DhtConfig, Endpoint, Events, InfraRoles, LookupBuilderFn, RelayServerConfig,
    Router, SecretKey, WebRtcP2pConfig, presets,
};

use crate::device::{DeviceName, OsInfo};
use crate::error::{AppError, AppResult};
use crate::host::{DeviceConfig, EventBus, Notifier, PairedDeviceStore};
use crate::network::CandidateScope;
use crate::network::NetManager;
use crate::network::config::{
    NetworkRuntimeConfig, bootstrap_node_addrs, create_candidate_manager,
};
use crate::pairing::{PairingManager, PairingService};
use crate::presence::OnlineRecordLookup;
use crate::protocol::{
    IDENTIFY_PROTOCOL, PAIRING, PAIRING_PROTOCOL, TRANSFER_CTRL, TRANSFER_CTRL_PROTOCOL,
    TRANSFER_DATA_PROTOCOL,
};
use crate::transfer::incoming::TransferCtrlService;
use crate::transfer::manager::TransferManager;
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

/// 启动 P2P 节点并装配 core 网络管理器与协议路由。
///
/// 设备名从 `device_config` 端口读出后塞入 `OsInfo.name`，经 identify 的 `agent_version`
/// 广播给对端，同时随 `OsInfo` 一路传到 `PairingManager`（配对请求与邀请串的 display）。
/// **本机 `OsInfo` 的装配点只此一处**——宿主传不进 `name`（`OsInfo::native()` 已去参）。
/// 这里装配的是**初值**：运行期改名走
/// [`device_name::rename_device`](crate::device_name::rename_device)，它同时更新
/// `PairingManager` 的本机 `OsInfo` 与 identify 的 `agent_version`，不重启节点。
///
/// 已配对设备的事实源是 `paired_device_store` 这个端口，本函数内部 load 一次——调用方
/// **不再预先加载、也不再传快照**。（此前桌面还在快照之上叠了一层「keychain 为空则回退
/// 前端 IPC 传来的列表」的静默兜底，那份前端列表本身就是后端的镜像。）
///
/// keychain 存量为 protobuf 编码，[`SecretKey`] 与之完全兼容。
// 依赖注入组合根：参数都是三端各自供给的端口 / 身份 / 配置（secret / os_info /
// device_config / paired / network_config / profile / event_bus / notifier / invite_store /
// transfer 工厂），打包成 struct 只是把同一组必填项换个容器、并不减少调用方负担，故直接放行。
#[expect(
    clippy::too_many_arguments,
    reason = "依赖注入组合根，参数都是三端必填的端口/身份/配置，打包成 struct 不减负担"
)]
pub async fn start_node<F>(
    secret_key: SecretKey,
    webrtc_certificate_pem: Option<String>,
    os_info: OsInfo,
    device_config: Arc<dyn DeviceConfig>,
    paired_device_store: Arc<dyn PairedDeviceStore>,
    network_config: NetworkRuntimeConfig,
    profile: EndpointProfile,
    event_bus: Arc<dyn EventBus>,
    notifier: Option<Arc<dyn Notifier>>,
    // 邀请注册表的落盘端口（native = SqlInviteStore / wasm = IndexedDB 实现）。
    // 不需要持久化时传 Arc::new(NoopInviteStore) —— 退回「重启丢邀请」的旧语义。
    invite_store: Arc<dyn InviteStore>,
    create_transfer: F,
) -> AppResult<StartedNode>
where
    F: FnOnce(Endpoint) -> TransferManager,
{
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

    let endpoint = build_endpoint(
        secret_key,
        webrtc_certificate_pem,
        agent_version,
        &network_config,
        profile,
    )
    .await?;

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
        event_bus,
        notifier.clone(),
        invite_store,
        paired_device_store,
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
    secret_key: SecretKey,
    webrtc_certificate_pem: Option<String>,
    agent_version: String,
    config: &NetworkRuntimeConfig,
    profile: EndpointProfile,
) -> AppResult<Endpoint> {
    let dht_config = DhtConfig {
        server_mode: config.provide_lan_helper,
        ..DhtConfig::default()
    };
    let mut builder = Endpoint::builder()
        .secret_key(secret_key)
        .identify_protocol(IDENTIFY_PROTOCOL)
        .agent_version(agent_version)
        .dht(dht_config);

    if let Some(pem) = webrtc_certificate_pem {
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

    // WebRTC 打洞：**两端都开**。
    //
    // 浏览器非开不可——它没有 DCUtR（那需要直连 socket，wasm 编译期就不存在），经 relay
    // 换信令是它把中转升级成直连的唯一途径。而原生端也必须开，因为**打洞要两端都支持**：
    // 只有浏览器开着，`web ↔ NAT 后的桌面/手机` 这一格照样是全程中转，而那恰恰是自研这个
    // 传输最想拿下的场景。
    //
    // 对原生端不是冗余：dcutr 走 TCP/QUIC 直连，ICE 走 UDP + STUN 候选，两者覆盖的 NAT
    // 类型不同，互为补充。
    builder = builder.webrtc_p2p(WebRtcP2pConfig {
        stun_servers: STUN_SERVERS.iter().map(|s| s.to_string()).collect(),
        ..WebRtcP2pConfig::default()
    });

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归锚点：Native profile 装配（TCP/QUIC listen + DHT lookup）能 bind——等价泛化前
    /// 的硬编码路径。Browser profile（空 listen + relay client、无 lookup/infra）也能 bind，
    /// 结构上验证「同一 `build_endpoint` 吃两种形态」。
    #[tokio::test]
    async fn both_profiles_bind() {
        let config = NetworkRuntimeConfig::default();

        let native = build_endpoint(
            SecretKey::generate(),
            None,
            "swarmdrop/test".to_string(),
            &config,
            EndpointProfile::Native,
        )
        .await
        .expect("native profile bind");
        native.close().await;

        let browser = build_endpoint(
            SecretKey::generate(),
            None,
            "swarmdrop/test".to_string(),
            &config,
            EndpointProfile::Browser,
        )
        .await
        .expect("browser profile bind");
        browser.close().await;
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

    #[tokio::test]
    async fn native_profile_publishes_webrtc_direct_address() {
        let native = build_endpoint(
            SecretKey::generate(),
            None,
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
                SecretKey::generate(),
                Some(pem.clone()),
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
