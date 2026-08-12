//! 应用生命周期 / 网络运行时 / 设备列表 / 应用更新等"杂项"IPC 命令
//!
//! 把和具体业务（identity / pairing / transfer / mcp）无关的命令集中放在这里，
//! 避免散落到 [`commands.rs`](super) 顶层。

use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swarmdrop_core::event_adapter::CoreTransferEvents;
use swarmdrop_core::host::{
    DeviceConfig, EventBus, FileAccess, Notifier, UpdateInstallRequest, UpdateInstaller,
};
use swarmdrop_core::transfer::manager::TransferManager;
use swarmdrop_net::SecretKey;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use crate::AppError;
use crate::database::TransferStoreState;
use crate::device::{DeviceFilter, DeviceListResult, OsInfo};
use crate::host::event_bus::TauriEventBus;
use crate::network::{NetManagerState, NetworkStatus};
use swarmdrop_core::network::NetworkRuntimeConfig;

#[tauri::command]
#[specta::specta]
pub async fn start(
    app: AppHandle,
    secret_key: State<'_, SecretKey>,
    network_options: Option<NetworkRuntimeConfig>,
) -> crate::AppResult<()> {
    // 准备 host adapters（在 NetManager 构造前必须就绪）
    let event_bus_struct = if let Some(bus) = app.try_state::<TauriEventBus>() {
        bus.inner().clone()
    } else {
        let bus = TauriEventBus::new(app.clone());
        app.manage(bus.clone());
        bus
    };
    let event_bus: Arc<dyn EventBus> = Arc::new(event_bus_struct);

    let db: Arc<DatabaseConnection> = app
        .try_state::<DatabaseConnection>()
        .map(|s| Arc::new(s.inner().clone()))
        .ok_or_else(|| AppError::transfer("数据库未初始化"))?;

    // 传输域持久化端口取自组装点托管的那一份，**不在这里新建**
    // （openspec: inbox-store-port-completion design D5）：注入 `TransferManager` 的
    // 与宿主自持的（收件箱命令 / MCP 工具 / 启动清理）是同一个 `Arc`，
    // 不是两个包装同一条连接的实例。
    let transfer_store: TransferStoreState = app
        .try_state::<TransferStoreState>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| AppError::transfer("传输存储未初始化"))?;

    // 与收件箱命令自持的**是同一个 `Arc`**（组装点在 `setup.rs`），不是两个包装同一个
    // AppHandle 的实例——`TauriFileAccess` 持有 `active_sinks`，两份实例意味着两张 sink 表。
    let file_access: Arc<dyn FileAccess> = app
        .try_state::<Arc<dyn FileAccess>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| AppError::transfer("文件访问端口未初始化"))?;

    let event_bus_for_factory = event_bus.clone();
    let file_access_for_factory = file_access.clone();

    let keychain = crate::host::keychain_provider(&app)?;
    let webrtc_certificate_pem =
        swarmdrop_core::identity::load_or_create_webrtc_certificate(&*keychain).await?;
    // host 只供给平台探测部分（hostname / os / platform / arch）；用户设备名由 core 的
    // 组合根从下面这个 `DeviceConfig` 端口读，桌面这侧没有 API 能把 name 塞进 OsInfo。
    let os_info = OsInfo::default();
    let device_config: Arc<dyn DeviceConfig> = Arc::new(crate::host::device_config(&app)?);
    // bootstrap_nodes 由桌面 host 注入；核心不再持有任何公共基础设施地址。
    let network_config = network_options.unwrap_or_default();

    // notifier 交给 core 的 RPC handler（pairing / transfer offer 入站时弹通知）。
    let notifier: Arc<dyn Notifier> =
        Arc::new(crate::host::notifier::DesktopNotifier::new(app.clone()));

    let started = swarmdrop_core::runtime::start_node(
        swarmdrop_core::runtime::NodeCredentials {
            secret_key: (*secret_key).clone(),
            webrtc_certificate_pem: Some(webrtc_certificate_pem),
            // 带证书持久化 ⇒ 桌面端监听 WebTransport（浏览器直连本机的快通道，回环实测是
            // webrtc-direct 的 4.5 倍）。
            webtransport: Some(crate::host::webtransport_config(&app)?),
        },
        os_info,
        network_config,
        swarmdrop_core::runtime::EndpointProfile::Native,
        swarmdrop_core::runtime::HostPorts {
            device_config,
            paired_device_store: crate::host::paired_device_store(&app)?,
            event_bus: event_bus.clone(),
            notifier: Some(notifier),
            // 邀请注册表落盘：24h TTL 下「发条链接给同事、自己顺手重启 App」是常态，
            // 内存态会让那条链接直接失效（openspec: invite-persistence）。
            invite_store: Arc::new(swarmdrop_storage_sql::SqlInviteStore::new((*db).clone())),
        },
        move |endpoint| {
            TransferManager::new(
                endpoint,
                Arc::new(CoreTransferEvents(event_bus_for_factory)),
                transfer_store,
                file_access_for_factory,
            )
        },
    )
    .await?;

    let net_manager = started.manager;
    let events = started.events;
    let router = started.router;

    // presence（宣告上线 / bootstrap / 已配对设备重连与保活）由 core 的
    // 事件循环自动接管（见 swarmdrop_core::presence），host 不再手工编排。
    let shared = net_manager.shared_refs();

    // 存入 Tauri state；已有旧节点（webview 重载后重复 start）先关停，
    // 否则旧 NetManager 被静默覆盖，其 cancel_token 永不触发，
    // presence/infra 循环与旧 swarm 永久泄漏
    if let Some(state) = app.try_state::<NetManagerState>() {
        let mut guard = state.lock().await;
        if let Some(old) = guard.as_ref() {
            old.shutdown().await;
        }
        *guard = Some(net_manager);
    } else {
        app.manage(Mutex::new(Some(net_manager)));
    }

    // 节点已启动 → 托盘进入在线态（新 TransferManager 默认未暂停）。
    crate::tray::refresh_tray(&app, true, false);

    crate::network::spawn_event_loop(events, shared, event_bus, router);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn shutdown(app: AppHandle) -> crate::AppResult<()> {
    if let Some(state) = app.try_state::<NetManagerState>() {
        let mut guard = state.lock().await;
        if let Some(manager) = guard.as_ref() {
            // 宣布下线 + 取消 presence 等后台任务
            manager.shutdown().await;
        }
        guard.take();
    }
    // 节点已停止 → 托盘进入离线态。
    crate::tray::refresh_tray(&app, false, false);
    Ok(())
}

/// 真正退出应用。
///
/// 关闭语义由前端 `onCloseRequested` 拦截：`closeBehavior=quit` 或首次对话框选「退出」
/// 时由前端显式调用本命令，确保进程退出（仅 `hide()` 不退出；macOS 关最后一个窗口默认
/// 也不退出）。托盘「退出」走 Rust 侧 `app.exit(0)`，不经本命令。
#[tauri::command]
#[specta::specta]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
#[specta::specta]
pub async fn list_devices(
    net: State<'_, NetManagerState>,
    filter: Option<DeviceFilter>,
) -> crate::AppResult<DeviceListResult> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or(AppError::node_not_started())?;
    let devices = manager.devices().get_devices(filter.unwrap_or_default());
    let total = devices.len();
    Ok(DeviceListResult { devices, total })
}

#[tauri::command]
#[specta::specta]
pub async fn get_network_status(
    net: State<'_, NetManagerState>,
) -> crate::AppResult<NetworkStatus> {
    let guard = net.lock().await;
    match guard.as_ref() {
        Some(manager) => Ok(manager.get_network_status()),
        None => Ok(NetworkStatus::default()),
    }
}

/// 下载并安装应用更新（桌面端）
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: AppHandle, url: String, is_force: bool) -> crate::AppResult<()> {
    let installer = crate::host::update_installer::DesktopUpdateInstaller::new(app);
    installer
        .install_update(UpdateInstallRequest { url, is_force })
        .await?;
    Ok(())
}
