//! Tauri IPC 命令入口
//!
//! 薄层命令入口，仅负责 Tauri 状态读取和参数解析，
//! 所有业务逻辑委托给 [`network`](crate::network)、
//! [`device`](crate::device) 和 [`pairing`](crate::pairing) 模块。

use std::sync::Arc;

use crate::device::{DeviceFilter, DeviceListResult, PairedDeviceInfo};
use crate::host::event_bus::TauriEventBus;
use crate::host::keychain::DesktopKeychainProvider;
use crate::network::{NetManagerState, NetworkStatus};
use crate::AppError;
use sea_orm::DatabaseConnection;
use swarm_p2p_core::libp2p::identity::Keypair;
use swarmdrop_core::host::{EventBus, FileAccess, UpdateInstallRequest, UpdateInstaller};
use swarmdrop_core::transfer::manager::TransferManager;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// 从 NetManagerState 获取 manager 引用并执行表达式（短暂持锁）
macro_rules! with_manager {
    ($net:expr, |$m:ident| $body:expr) => {{
        let guard = $net.lock().await;
        let $m = guard
            .as_ref()
            .ok_or_else(|| $crate::AppError::NodeNotStarted)?;
        Ok::<_, $crate::AppError>($body?)
    }};
}

mod identity;
mod mcp;
mod pairing;
mod transfer;

// glob re-export：Tauri 的 #[tauri::command] 宏会生成 __cmd__* 隐藏符号，
// generate_handler! 需要通过模块路径访问这些符号，显式导出无法覆盖。
pub use identity::*;
pub use mcp::*;
pub use pairing::*;
pub use transfer::*;

#[tauri::command]
pub async fn start(
    app: AppHandle,
    keypair: State<'_, Keypair>,
    paired_devices: Vec<PairedDeviceInfo>,
    custom_bootstrap_nodes: Option<Vec<String>>,
) -> crate::AppResult<()> {
    let paired_devices = load_host_paired_devices(paired_devices).await?;

    // 准备 host adapters（在 NetManager 构造前必须就绪）
    let event_bus_struct = TauriEventBus::new(app.clone());
    // 注册到 app state，供 commands::prepare_send 等调用方使用 channel 路由
    if app.try_state::<TauriEventBus>().is_none() {
        app.manage(event_bus_struct.clone());
    }
    let event_bus: Arc<dyn EventBus> = Arc::new(event_bus_struct);

    let db: Arc<DatabaseConnection> = app
        .try_state::<DatabaseConnection>()
        .map(|s| Arc::new(s.inner().clone()))
        .ok_or_else(|| AppError::Transfer("数据库未初始化".into()))?;

    let file_access: Arc<dyn FileAccess> =
        Arc::new(crate::file_source::TauriFileAccess::new(app.clone()));

    let event_bus_for_factory = event_bus.clone();
    let db_for_factory = db.clone();
    let file_access_for_factory = file_access.clone();

    let started = swarmdrop_core::runtime::start_node(
        (*keypair).clone(),
        paired_devices,
        custom_bootstrap_nodes.unwrap_or_default(),
        move |client| {
            TransferManager::new(
                client,
                event_bus_for_factory,
                db_for_factory,
                file_access_for_factory,
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

    // 获取事件循环需要的共享引用（在存入 state 之前）
    let shared = net_manager.shared_refs();

    // DHT bootstrap → 完成后检查已配对设备是否在线
    let bootstrap_client = client.clone();
    let pairing_for_startup = shared.pairing.clone();
    tokio::spawn(async move {
        match bootstrap_client.bootstrap().await {
            Ok(result) => info!("DHT bootstrap completed: {:?}", result),
            Err(e) => warn!("DHT bootstrap failed: {}", e),
        }
        // bootstrap 完成后，查询已配对设备的在线记录并注册地址
        pairing_for_startup.check_paired_online().await;
    });

    // 存入 Tauri state
    if let Some(state) = app.try_state::<NetManagerState>() {
        *state.lock().await = Some(net_manager);
    } else {
        app.manage(Mutex::new(Some(net_manager)));
    }

    // 启动事件循环
    crate::network::spawn_event_loop(receiver, app, shared, event_bus);

    Ok(())
}

async fn load_host_paired_devices(
    fallback: Vec<PairedDeviceInfo>,
) -> crate::AppResult<Vec<PairedDeviceInfo>> {
    let provider = DesktopKeychainProvider::new()?;
    let devices = swarmdrop_core::identity::load_paired_devices(&provider).await?;
    if devices.is_empty() {
        Ok(fallback)
    } else {
        Ok(devices)
    }
}

#[tauri::command]
pub async fn shutdown(app: AppHandle) -> crate::AppResult<()> {
    if let Some(state) = app.try_state::<NetManagerState>() {
        let mut guard = state.lock().await;
        if let Some(manager) = guard.as_ref() {
            if let Err(e) = manager.pairing().announce_offline().await {
                warn!("Failed to announce offline: {}", e);
            }
            // 取消所有后台任务（超时清理等）
            manager.cancel_background_tasks();
        }
        guard.take();
    }

    Ok(())
}

#[tauri::command]
pub async fn list_devices(
    net: State<'_, NetManagerState>,
    filter: Option<DeviceFilter>,
) -> crate::AppResult<DeviceListResult> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or(AppError::NodeNotStarted)?;
    let devices = manager.devices().get_devices(filter.unwrap_or_default());
    let total = devices.len();
    Ok(DeviceListResult { devices, total })
}

#[tauri::command]
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
pub async fn install_update(app: AppHandle, url: String, is_force: bool) -> crate::AppResult<()> {
    let installer = crate::host::update_installer::DesktopUpdateInstaller::new(app);
    installer
        .install_update(UpdateInstallRequest { url, is_force })
        .await?;
    Ok(())
}
