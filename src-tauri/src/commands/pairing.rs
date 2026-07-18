use crate::AppResult;
use crate::device::DeviceFilter;
use crate::events::{DevicesChanged, PairedDeviceAdded};
use crate::network::NetManagerState;
use serde::{Deserialize, Serialize};
use swarmdrop_core::device::{DeviceReceivePolicy, DeviceTrustLevel, PairedDeviceInfo};
use swarmdrop_core::pairing::code::{PairingCodeInfo, ShareCodeRecord};
use swarmdrop_core::protocol::{PairingMethod, PairingResponse};
use swarmdrop_net::{Addr, NodeId};
use tauri::{AppHandle, State};
use tauri_specta::Event as _;

use crate::AppError;

/// 把前端传来的 base58 字符串解析为 [`NodeId`]，失败归一化为 identity 错误。
fn parse_peer_id(peer_id: &str) -> AppResult<NodeId> {
    peer_id
        .parse()
        .map_err(|e| AppError::identity(format!("invalid peer_id: {e}")))
}

/// 查询设备信息的返回类型
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// 节点身份 NodeId（base58 字符串）
    pub peer_id: String,
    pub code_record: ShareCodeRecord,
}

/// 生成配对码
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub async fn get_device_info(
    net: State<'_, NetManagerState>,
    code: String,
) -> AppResult<DeviceInfo> {
    let (peer_id, code_record) = with_manager!(net, |m| m.pairing().get_device_info(&code).await)?;
    Ok(DeviceInfo {
        peer_id: peer_id.to_string(),
        code_record,
    })
}

/// 向对端发起配对请求
///
/// 配对成功后自动添加到已配对设备，并 emit `paired-device-added` 事件通知前端。
///
/// `peer_id` 为 base58 字符串，`addrs` 为 multiaddr 字符串列表，由命令内部解析为
/// 内核 newtype，方便通过 specta 生成 TypeScript bindings（内核类型本身不实现
/// `specta::Type`）。
#[tauri::command]
#[specta::specta]
pub async fn request_pairing(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    peer_id: String,
    method: PairingMethod,
    addrs: Option<Vec<String>>,
) -> AppResult<PairingResponse> {
    let peer_id = parse_peer_id(&peer_id)?;
    let addrs = addrs
        .map(|list| {
            list.into_iter()
                .map(|s| s.parse::<Addr>())
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|e| AppError::identity(format!("invalid multiaddr: {e}")))?;

    let (response, paired_info) = with_manager!(net, |m| m
        .pairing()
        .request_pairing(peer_id, method, addrs)
        .await)?;

    if let Some(info) = paired_info {
        persist_paired_device(&app, info.clone()).await?;
        let _ = PairedDeviceAdded(info).emit(&app);
        publish_devices_changed(&app, &net).await;
    }

    Ok(response)
}

/// 取消与指定设备的配对（同步更新运行时状态）
///
/// `peer_id` 为 base58 字符串，由命令内部解析为 `NodeId`。
#[tauri::command]
#[specta::specta]
pub async fn remove_paired_device(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    peer_id: String,
) -> AppResult<()> {
    let peer_id = parse_peer_id(&peer_id)?;
    let guard = net.lock().await;
    // 节点未运行时仍更新 host keychain 中的持久化列表。
    if let Some(manager) = guard.as_ref() {
        manager.pairing().remove_paired_device(&peer_id);
    }
    drop(guard);
    persist_paired_device_removal(&app, &peer_id).await?;
    publish_devices_changed(&app, &net).await;
    Ok(())
}

/// 更新已配对设备的可信策略。
#[tauri::command]
#[specta::specta]
pub async fn update_paired_device_policy(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    peer_id: String,
    trust_level: DeviceTrustLevel,
    receive_policy: Option<DeviceReceivePolicy>,
) -> AppResult<PairedDeviceInfo> {
    let peer_id = parse_peer_id(&peer_id)?;
    let provider = crate::host::keychain_provider(&app)?;
    let devices = swarmdrop_core::identity::update_paired_device_policy(
        &*provider,
        &peer_id,
        trust_level,
        receive_policy,
    )
    .await?;
    let updated = devices
        .into_iter()
        .find(|device| device.peer_id == peer_id)
        .ok_or_else(|| AppError::identity("未找到已配对设备".to_string()))?;

    let guard = net.lock().await;
    if let Some(manager) = guard.as_ref() {
        manager.pairing().add_paired_device(updated.clone());
    }
    drop(guard);

    publish_devices_changed(&app, &net).await;
    Ok(updated)
}

/// 处理收到的配对请求（接受/拒绝）
///
/// 接受配对后自动添加到已配对设备，并 emit `paired-device-added` 事件通知前端。
#[tauri::command]
#[specta::specta]
pub async fn respond_pairing_request(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    pending_id: u64,
    method: PairingMethod,
    response: PairingResponse,
) -> AppResult<()> {
    // 新内核里配对方式已随入站请求缓存在 core 的 pending 表，respond 无需回传；
    // 保留 `method` 参数仅为 IPC 签名稳定（避免前端 bindings 变更）。
    let _ = method;
    let paired_info = with_manager!(net, |m| {
        m.pairing()
            .respond_pairing_request(pending_id, response)
            .await
    })?;

    if let Some(info) = paired_info {
        persist_paired_device(&app, info.clone()).await?;
        let _ = PairedDeviceAdded(info).emit(&app);
        publish_devices_changed(&app, &net).await;
    }

    Ok(())
}

async fn persist_paired_device(
    app: &AppHandle,
    info: crate::device::PairedDeviceInfo,
) -> AppResult<()> {
    let provider = crate::host::keychain_provider(app)?;
    swarmdrop_core::identity::upsert_paired_device(&*provider, info).await?;
    Ok(())
}

async fn persist_paired_device_removal(app: &AppHandle, peer_id: &NodeId) -> AppResult<()> {
    let provider = crate::host::keychain_provider(app)?;
    swarmdrop_core::identity::remove_paired_device(&*provider, peer_id).await?;
    Ok(())
}

async fn publish_devices_changed(app: &AppHandle, net: &State<'_, NetManagerState>) {
    let guard = net.lock().await;
    let Some(manager) = guard.as_ref() else {
        return;
    };
    let devices = manager.devices().get_devices(DeviceFilter::All);
    let _ = DevicesChanged(devices).emit(app);
}
