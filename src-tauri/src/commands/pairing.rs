use crate::events;
use crate::network::NetManagerState;
use crate::pairing::code::{PairingCodeInfo, ShareCodeRecord};
use crate::protocol::{PairingMethod, PairingResponse};
use crate::AppResult;
use serde::{Deserialize, Serialize};
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};
use tauri::{AppHandle, Emitter, State};

#[cfg(not(target_os = "android"))]
use crate::host::keychain::DesktopKeychainProvider;

/// 查询设备信息的返回类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub peer_id: PeerId,
    pub code_record: ShareCodeRecord,
}

/// 生成配对码
#[tauri::command]
pub async fn generate_pairing_code(
    net: State<'_, NetManagerState>,
    expires_in_secs: Option<u64>,
) -> AppResult<PairingCodeInfo> {
    with_manager!(net, |m| m
        .pairing()
        .generate_code(expires_in_secs.unwrap_or(300))
        .await)
}

/// 通过配对码查询对端设备信息
#[tauri::command]
pub async fn get_device_info(
    net: State<'_, NetManagerState>,
    code: String,
) -> AppResult<DeviceInfo> {
    let (peer_id, code_record) = with_manager!(net, |m| m.pairing().get_device_info(&code).await)?;
    Ok(DeviceInfo {
        peer_id,
        code_record,
    })
}

/// 向对端发起配对请求
///
/// 配对成功后自动添加到已配对设备，并 emit `paired-device-added` 事件通知前端。
#[tauri::command]
pub async fn request_pairing(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    peer_id: PeerId,
    method: PairingMethod,
    addrs: Option<Vec<Multiaddr>>,
) -> AppResult<PairingResponse> {
    let (response, paired_info) = with_manager!(net, |m| m
        .pairing()
        .request_pairing(peer_id, method, addrs)
        .await)?;

    if let Some(info) = paired_info {
        persist_paired_device(info.clone()).await?;
        let _ = app.emit(events::PAIRED_DEVICE_ADDED, &info);
    }

    Ok(response)
}

/// 取消与指定设备的配对（同步更新运行时状态）
#[tauri::command]
pub async fn remove_paired_device(
    net: State<'_, NetManagerState>,
    peer_id: PeerId,
) -> AppResult<()> {
    let guard = net.lock().await;
    // 节点未运行时仍更新 host keychain 中的持久化列表。
    if let Some(manager) = guard.as_ref() {
        manager.pairing().remove_paired_device(&peer_id);
    }
    persist_paired_device_removal(&peer_id).await?;
    Ok(())
}

/// 处理收到的配对请求（接受/拒绝）
///
/// 接受配对后自动添加到已配对设备，并 emit `paired-device-added` 事件通知前端。
#[tauri::command]
pub async fn respond_pairing_request(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    pending_id: u64,
    method: PairingMethod,
    response: PairingResponse,
) -> AppResult<()> {
    let paired_info = with_manager!(net, |m| {
        m.pairing()
            .handle_pairing_request(pending_id, &method, response)
            .await
    })?;

    if let Some(info) = paired_info {
        persist_paired_device(info.clone()).await?;
        let _ = app.emit(events::PAIRED_DEVICE_ADDED, &info);
    }

    Ok(())
}

async fn persist_paired_device(info: crate::device::PairedDeviceInfo) -> AppResult<()> {
    #[cfg(not(target_os = "android"))]
    {
        let provider = DesktopKeychainProvider::new()?;
        swarmdrop_core::identity::upsert_paired_device(&provider, info).await?;
    }

    #[cfg(target_os = "android")]
    {
        let _ = info;
    }

    Ok(())
}

async fn persist_paired_device_removal(peer_id: &PeerId) -> AppResult<()> {
    #[cfg(not(target_os = "android"))]
    {
        let provider = DesktopKeychainProvider::new()?;
        swarmdrop_core::identity::remove_paired_device(&provider, peer_id).await?;
    }

    #[cfg(target_os = "android")]
    {
        let _ = peer_id;
    }

    Ok(())
}
