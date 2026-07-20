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

/// 启动 P2P 节点并装配 core 网络管理器与协议路由。
///
/// `device_name` 来自 host 持久化层（桌面端的 device_config.json / 移动端的 RN
/// settings），节点启动时塞入 `OsInfo.name`，通过 identify 的 `agent_version` 字段
/// 广播给对端。Host 改名后需 stop + start 节点让新值上线。
///
/// keychain 存量为 protobuf 编码，[`SecretKey`] 与之完全兼容。
pub async fn start_node<F>(
    secret_key: SecretKey,
    device_name: Option<String>,
    paired_devices: Vec<PairedDeviceInfo>,
    network_config: NetworkRuntimeConfig,
    event_bus: Arc<dyn EventBus>,
    notifier: Option<Arc<dyn Notifier>>,
    create_transfer: F,
) -> AppResult<StartedNode>
where
    F: FnOnce(Endpoint) -> TransferManager,
{
    let os_info = OsInfo {
        name: device_name,
        ..OsInfo::default()
    };
    let os_info = if network_config.provide_lan_helper {
        os_info.with_capability(OsInfo::LAN_HELPER_CAPABILITY)
    } else {
        os_info
    };
    let agent_version = os_info.to_agent_version();

    let endpoint = build_endpoint(secret_key, agent_version, &network_config).await?;

    // 注册引导/中继基础设施节点（DHT bootstrap 依赖至少一个 kad server 进路由表）。
    for peer in bootstrap_node_addrs(&network_config) {
        if let Err(e) = endpoint
            .add_infrastructure_peer(peer, InfraRoles::bootstrap())
            .await
        {
            tracing::warn!("注册引导节点失败: {e}");
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

/// 按 [`NetworkRuntimeConfig`] 装配并 bind 一个原生 Endpoint。
///
/// LanHelper 模式 = relay server 开 + DHT server_mode（本机已知可达）；否则 DHT
/// server_mode 由 AutoNAT 判定。在线记录 lookup 用 [`LookupBuilderFn`] 延迟构造
/// （构造依赖已 bind 的 Endpoint）。
async fn build_endpoint(
    secret_key: SecretKey,
    agent_version: String,
    config: &NetworkRuntimeConfig,
) -> AppResult<Endpoint> {
    let dht_config = DhtConfig {
        server_mode: config.provide_lan_helper,
        ..DhtConfig::default()
    };
    let mut builder = Endpoint::builder()
        .secret_key(secret_key)
        .preset(presets::Native)
        .identify_protocol(IDENTIFY_PROTOCOL)
        .agent_version(agent_version)
        .dht(dht_config)
        .address_lookup(LookupBuilderFn(|ep: &Endpoint| {
            Ok(Box::new(OnlineRecordLookup::new(ep.clone())) as Box<dyn AddressLookup>)
        }));
    if config.provide_lan_helper {
        builder = builder.relay_server(RelayServerConfig::default());
    }
    builder
        .bind()
        .await
        .map_err(|e| AppError::Network(e.to_string()))
}
