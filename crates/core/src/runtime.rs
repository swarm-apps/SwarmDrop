//! Core runtime 入口。
//!
//! 装配新内核：`Endpoint`（builder + preset + DHT + 在线记录 lookup）→ 注册引导节点 →
//! 建 `NetManager`（pairing / devices / presence / infra）→ 建 `Router`（pairing /
//! transfer-ctrl / transfer-data 三协议）→ 返回订阅句柄供事件循环消费。

use std::sync::Arc;

use swarmdrop_net::{
    AddressLookup, DhtConfig, Endpoint, Events, InfraRoles, LookupBuilderFn, RelayServerConfig,
    Router, SecretKey, presets,
};

use crate::device::{OsInfo, PairedDeviceInfo};
use crate::error::{AppError, AppResult};
use crate::host::{EventBus, Notifier};
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
/// `device_name` 来自 host 持久化层（桌面端的 device_config.json / 移动端的 RN
/// settings），节点启动时塞入 `OsInfo.name`，通过 identify 的 `agent_version` 字段
/// 广播给对端。Host 改名后需 stop + start 节点让新值上线。
///
/// keychain 存量为 protobuf 编码，[`SecretKey`] 与之完全兼容。
// 依赖注入组合根：8 个参数都是三端各自供给的端口 / 身份 / 配置（secret / os_info /
// paired / network_config / profile / event_bus / notifier / transfer 工厂），打包成
// struct 只是把同一组必填项换个容器、并不减少调用方负担，故直接放行。
#[expect(
    clippy::too_many_arguments,
    reason = "依赖注入组合根，8 个参数都是三端必填的端口/身份/配置，打包成 struct 不减负担"
)]
pub async fn start_node<F>(
    secret_key: SecretKey,
    os_info: OsInfo,
    paired_devices: Vec<PairedDeviceInfo>,
    network_config: NetworkRuntimeConfig,
    profile: EndpointProfile,
    event_bus: Arc<dyn EventBus>,
    notifier: Option<Arc<dyn Notifier>>,
    create_transfer: F,
) -> AppResult<StartedNode>
where
    F: FnOnce(Endpoint) -> TransferManager,
{
    // `os_info` 由 host 供给基础字段（native 走 `OsInfo::default()` 探测、wasm 走
    // `web_os_info()`——env 探测在 wasm 恒 unknown 故必须由调用方注入）。LAN Helper
    // 能力由 `network_config` 决定，仍在此处叠加，保持 `to_agent_version()` 契约。
    let os_info = if network_config.provide_lan_helper {
        os_info.with_capability(OsInfo::LAN_HELPER_CAPABILITY)
    } else {
        os_info
    };
    let agent_version = os_info.to_agent_version();

    let endpoint = build_endpoint(secret_key, agent_version, &network_config, profile).await?;

    // 注册引导/中继基础设施节点（DHT bootstrap 依赖至少一个 kad server 进路由表）。
    // 浏览器端无内置引导，整循环跳过。
    if profile.registers_infra() {
        for peer in bootstrap_node_addrs(&network_config) {
            if let Err(e) = endpoint
                .add_infrastructure_peer(peer, InfraRoles::bootstrap())
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
    let manager = NetManager::new(
        endpoint.clone(),
        paired_devices,
        transfer,
        network_config,
        candidate_manager,
        event_bus,
        notifier.clone(),
    );

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

/// 装配三协议入站 [`Router`] 并 spawn。
///
/// pairing 与 transfer 控制面是 typed RPC（[`PAIRING`] / [`TRANSFER_CTRL`]），数据面
/// （[`TRANSFER_DATA_PROTOCOL`]）是裸流。runtime 与 e2e 测试共用同一套装配，避免协议
/// 注册漂移。返回的 `Router` 需与 `endpoint` 同生命周期。
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
        .accept(TRANSFER_DATA_PROTOCOL, TransferDataHandler::new(transfer))
        .spawn()
}

/// 按 [`EndpointProfile`] + [`NetworkRuntimeConfig`] 装配并 bind 一个 Endpoint。
///
/// preset / DHT 在线记录 lookup 由 profile 决定（Native 挂 `OnlineRecordLookup`、Browser
/// 不挂）；LanHelper 模式 = relay server 开 + DHT server_mode（本机已知可达），否则 DHT
/// server_mode 由 AutoNAT 判定。在线记录 lookup 用 [`LookupBuilderFn`] 延迟构造（构造依赖
/// 已 bind 的 Endpoint）。identify 协议 / agent_version / DHT server_mode 三端一致。
async fn build_endpoint(
    secret_key: SecretKey,
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

    builder = match profile {
        EndpointProfile::Native => builder
            .preset(presets::Native)
            .address_lookup(LookupBuilderFn(|ep: &Endpoint| {
                Ok(Box::new(OnlineRecordLookup::new(ep.clone())) as Box<dyn AddressLookup>)
            })),
        EndpointProfile::Browser => builder.preset(presets::Browser),
    };

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
            "swarmdrop/test".to_string(),
            &config,
            EndpointProfile::Native,
        )
        .await
        .expect("native profile bind");
        native.close().await;

        let browser = build_endpoint(
            SecretKey::generate(),
            "swarmdrop/test".to_string(),
            &config,
            EndpointProfile::Browser,
        )
        .await
        .expect("browser profile bind");
        browser.close().await;
    }
}
