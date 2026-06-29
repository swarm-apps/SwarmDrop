//! 应用生命周期 / 网络运行时 / 设备列表 / 应用更新等"杂项"IPC 命令
//!
//! 把和具体业务（identity / pairing / transfer / mcp）无关的命令集中放在这里，
//! 避免散落到 [`commands.rs`](super) 顶层。

use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swarm_p2p_core::libp2p::identity::Keypair;
use swarmdrop_core::host::{EventBus, FileAccess, UpdateInstallRequest, UpdateInstaller};
use swarmdrop_core::transfer::manager::TransferManager;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::AppError;
use crate::device::{DeviceFilter, DeviceListResult, PairedDeviceInfo};
use crate::host::event_bus::TauriEventBus;
use crate::network::{NetManagerState, NetworkStatus};
use swarmdrop_core::network::NetworkRuntimeConfig;

#[tauri::command]
#[specta::specta]
pub async fn start(
    app: AppHandle,
    keypair: State<'_, Keypair>,
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
    // custom_bootstrap_nodes 现统一由 network_options 携带（前端 NetworkRuntimeConfig），
    // 不再有独立的 legacy 位置参与合并。
    let network_config = network_options.unwrap_or_default();

    let started = swarmdrop_core::runtime::start_node(
        (*keypair).clone(),
        device_name,
        paired_devices,
        network_config,
        move |client, dc_receiver| {
            TransferManager::new(
                client,
                event_bus_for_factory,
                db_for_factory,
                file_access_for_factory,
                dc_receiver,
            )
        },
    )?;

    let net_manager = started.manager;
    let receiver = started.receiver;
    let client = net_manager.client().clone();

    // 宣布上线（bootstrap 前发布，尽早让对方发现）
    if let Err(e) = net_manager.pairing().announce_online().await {
        warn!("Failed to announce online: {}", e);
    }

    let shared = net_manager.shared_refs();

    // DHT bootstrap → 完成后检查已配对设备是否在线
    let bootstrap_client = client.clone();
    let pairing_for_startup = shared.pairing.clone();
    tokio::spawn(async move {
        match bootstrap_client.bootstrap().await {
            Ok(result) => info!("DHT bootstrap completed: {:?}", result),
            Err(e) => warn!("DHT bootstrap failed: {}", e),
        }
        pairing_for_startup.check_paired_online().await;
    });

    // 存入 Tauri state
    if let Some(state) = app.try_state::<NetManagerState>() {
        *state.lock().await = Some(net_manager);
    } else {
        app.manage(Mutex::new(Some(net_manager)));
    }

    crate::network::spawn_event_loop(receiver, app, shared, event_bus);

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
            if let Err(e) = manager.pairing().announce_offline().await {
                warn!("Failed to announce offline: {}", e);
            }
            manager.cancel_background_tasks();
        }
        guard.take();
    }
    Ok(())
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
