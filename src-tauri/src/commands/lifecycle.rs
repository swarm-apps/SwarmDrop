//! 应用生命周期 / 网络运行时 / 设备列表 / 应用更新等"杂项"IPC 命令
//!
//! 把和具体业务（identity / pairing / transfer / mcp）无关的命令集中放在这里，
//! 避免散落到 [`commands.rs`](super) 顶层。

use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swarmdrop_core::event_adapter::CoreTransferEvents;
use swarmdrop_core::host::{EventBus, FileAccess, Notifier, UpdateInstallRequest, UpdateInstaller};
use swarmdrop_core::transfer::manager::TransferManager;
use swarmdrop_net::SecretKey;
use swarmdrop_storage_sql::SqlSessionStore;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use crate::AppError;
use crate::device::{DeviceFilter, DeviceListResult, OsInfo, PairedDeviceInfo};
use crate::host::event_bus::TauriEventBus;
use crate::network::{NetManagerState, NetworkStatus};
use swarmdrop_core::network::NetworkRuntimeConfig;

#[tauri::command]
#[specta::specta]
pub async fn start(
    app: AppHandle,
    secret_key: State<'_, SecretKey>,
    paired_devices: Vec<PairedDeviceInfo>,
    network_options: Option<NetworkRuntimeConfig>,
) -> crate::AppResult<()> {
    let paired_devices = load_host_paired_devices(&app, paired_devices).await?;

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

    let file_access: Arc<dyn FileAccess> =
        Arc::new(crate::host::file_source::TauriFileAccess::new(app.clone()));

    let event_bus_for_factory = event_bus.clone();
    let db_for_factory = db.clone();
    let file_access_for_factory = file_access.clone();

    let device_name = crate::host::device_config::load_device_name(&app).await;
    // os_info 由 host 供给：桌面走 `OsInfo::native` 的 env 探测（hostname/os/arch）+ 用户设备名。
    let os_info = OsInfo::native(device_name);
    // custom_bootstrap_nodes 现统一由 network_options 携带（前端 NetworkRuntimeConfig），
    // 不再有独立的 legacy 位置参与合并。
    let network_config = network_options.unwrap_or_default();

    // notifier 交给 core 的 RPC handler（pairing / transfer offer 入站时弹通知）。
    let notifier: Arc<dyn Notifier> =
        Arc::new(crate::host::notifier::DesktopNotifier::new(app.clone()));

    let started = swarmdrop_core::runtime::start_node(
        (*secret_key).clone(),
        os_info,
        paired_devices,
        network_config,
        swarmdrop_core::runtime::EndpointProfile::Native,
        event_bus.clone(),
        Some(notifier),
        move |endpoint| {
            TransferManager::new(
                endpoint,
                Arc::new(CoreTransferEvents(event_bus_for_factory)),
                Arc::new(SqlSessionStore::new(db_for_factory)),
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

async fn load_host_paired_devices(
    app: &AppHandle,
    fallback: Vec<PairedDeviceInfo>,
) -> crate::AppResult<Vec<PairedDeviceInfo>> {
    let provider = crate::host::keychain_provider(app)?;
    let devices = swarmdrop_core::identity::load_paired_devices(&*provider).await?;
    if devices.is_empty() {
        Ok(fallback)
    } else {
        Ok(devices)
    }
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
