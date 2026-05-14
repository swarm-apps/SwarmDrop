//! Tauri IPC 命令入口
//!
//! 薄层命令入口，仅负责 Tauri 状态读取和参数解析，
//! 所有业务逻辑委托给 [`network`](crate::network)、
//! [`device`](crate::device) 和 [`pairing`](crate::pairing) 模块。

use crate::device::{DeviceFilter, DeviceListResult, PairedDeviceInfo};
use crate::network::{NetManagerState, NetworkStatus};
use crate::transfer::offer::TransferManager;
use crate::AppError;
use swarm_p2p_core::libp2p::identity::Keypair;
use swarmdrop_core::host::{UpdateInstallRequest, UpdateInstaller};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use tracing::{info, warn};

#[cfg(not(target_os = "android"))]
use crate::host::keychain::DesktopKeychainProvider;

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

    let started = swarmdrop_core::runtime::start_node(
        (*keypair).clone(),
        paired_devices,
        custom_bootstrap_nodes.unwrap_or_default(),
        TransferManager::new,
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
    crate::network::spawn_event_loop(receiver, app, shared);

    Ok(())
}

async fn load_host_paired_devices(
    fallback: Vec<PairedDeviceInfo>,
) -> crate::AppResult<Vec<PairedDeviceInfo>> {
    #[cfg(not(target_os = "android"))]
    {
        let provider = DesktopKeychainProvider::new()?;
        let devices = swarmdrop_core::identity::load_paired_devices(&provider).await?;
        if !devices.is_empty() {
            return Ok(devices);
        }
    }

    Ok(fallback)
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

/// Android APK 下载安装（仅 Android 平台可用）
#[tauri::command]
pub async fn install_update(app: AppHandle, url: String, is_force: bool) -> crate::AppResult<()> {
    #[cfg(target_os = "android")]
    {
        let updater = app.state::<crate::mobile::UpdaterPlugin<tauri::Wry>>();
        updater.install_update(url, is_force)?;
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let installer = crate::host::update_installer::DesktopUpdateInstaller::new(app);
        installer
            .install_update(UpdateInstallRequest { url, is_force })
            .await?;
        Ok(())
    }
}
