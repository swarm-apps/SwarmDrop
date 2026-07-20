use swarmdrop_core::device::PairedDeviceInfo;
use swarmdrop_core::host::DeviceIdentityBytes;
use swarmdrop_net::SecretKey;
use tauri::{AppHandle, Manager};

use crate::AppResult;

#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IdentityState {
    pub keypair: Vec<u8>,
    pub device_id: String,
    pub paired_devices: Vec<PairedDeviceInfo>,
    pub created: bool,
}

/// 从系统 keychain 初始化设备身份，不再要求用户输入 Stronghold 密码。
#[tauri::command]
#[specta::specta]
pub async fn initialize_identity(app: AppHandle) -> AppResult<IdentityState> {
    let provider = crate::host::keychain_provider(&app)?;
    let identity = swarmdrop_core::identity::load_or_create_identity(&*provider).await?;
    let paired_devices = provider.load_paired_devices().await?;
    let device_id = identity.node_id.to_string();

    app.manage(identity.secret_key);

    Ok(IdentityState {
        keypair: identity.keypair_bytes,
        device_id,
        paired_devices,
        created: identity.created,
    })
}

/// 生成新的 Ed25519 密钥对（protobuf 编码，与存量 keychain 格式兼容）。
#[tauri::command]
#[specta::specta]
pub async fn generate_keypair() -> AppResult<Vec<u8>> {
    Ok(SecretKey::generate().to_protobuf())
}

/// 读取持久化的设备名（onboarding 完成前为 `None`）。
#[tauri::command]
#[specta::specta]
pub async fn get_device_name(app: AppHandle) -> AppResult<Option<String>> {
    Ok(crate::host::device_config::load_device_name(&app).await)
}

/// 设置设备名并持久化。
///
/// 仅写入 `device_config.json`。要让新名字通过 identify 协议的 `agent_version`
/// 重新广播，前端在本命令返回后自己调 `shutdown` + `start`（前端持有
/// paired_devices + network_options 上下文）。
///
/// `name = None`（或空串/纯空白）清空，回退到系统 hostname。
#[tauri::command]
#[specta::specta]
pub async fn set_device_name(app: AppHandle, name: Option<String>) -> AppResult<()> {
    let normalized = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    crate::host::device_config::save_device_name(&app, normalized)
        .await
        .map_err(|e| crate::AppError::identity(format!("save device name: {e}")))?;
    Ok(())
}

/// 注册密钥对到 Tauri state，并在桌面端写入系统 keychain。
#[tauri::command]
#[specta::specta]
pub async fn register_keypair(app: AppHandle, keypair: Vec<u8>) -> AppResult<String> {
    let secret_key =
        SecretKey::from_protobuf(&keypair).map_err(|e| crate::AppError::identity(e.to_string()))?;
    let node_id = secret_key.node_id();

    let provider = crate::host::keychain_provider(&app)?;
    provider
        .save_identity(DeviceIdentityBytes {
            keypair: secret_key.to_protobuf(),
        })
        .await?;

    app.manage(secret_key);

    Ok(node_id.to_string())
}
