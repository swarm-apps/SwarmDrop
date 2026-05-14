use swarm_p2p_core::libp2p::identity::Keypair;
use swarmdrop_core::device::PairedDeviceInfo;
use swarmdrop_core::host::{DeviceIdentityBytes, KeychainProvider};
use tauri::{AppHandle, Manager};

use crate::host::keychain::DesktopKeychainProvider;
use crate::AppResult;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityState {
    pub keypair: Vec<u8>,
    pub device_id: String,
    pub paired_devices: Vec<PairedDeviceInfo>,
    pub created: bool,
}

/// 从系统 keychain 初始化设备身份，不再要求用户输入 Stronghold 密码。
#[tauri::command]
pub async fn initialize_identity(app: AppHandle) -> AppResult<IdentityState> {
    let provider = DesktopKeychainProvider::new()?;
    let identity = swarmdrop_core::identity::load_or_create_identity(&provider).await?;
    let paired_devices = provider.load_paired_devices().await?;
    let device_id = identity.peer_id.to_string();

    app.manage(identity.keypair);

    Ok(IdentityState {
        keypair: identity.keypair_bytes,
        device_id,
        paired_devices,
        created: identity.created,
    })
}

/// 生成新的 Ed25519 密钥对。
#[tauri::command]
pub async fn generate_keypair() -> AppResult<Vec<u8>> {
    let keypair = Keypair::generate_ed25519();
    keypair
        .to_protobuf_encoding()
        .map_err(|e| crate::AppError::Identity(e.to_string()))
}

/// 注册密钥对到 Tauri state，并在桌面端写入系统 keychain。
#[tauri::command]
pub async fn register_keypair(app: AppHandle, keypair: Vec<u8>) -> AppResult<String> {
    let keypair = Keypair::from_protobuf_encoding(&keypair)
        .map_err(|e| crate::AppError::Identity(e.to_string()))?;
    let peer_id = keypair.public().to_peer_id();

    let provider = DesktopKeychainProvider::new()?;
    provider
        .save_identity(DeviceIdentityBytes {
            keypair: keypair
                .to_protobuf_encoding()
                .map_err(|e| crate::AppError::Identity(e.to_string()))?,
        })
        .await?;

    app.manage(keypair);

    Ok(peer_id.to_string())
}
