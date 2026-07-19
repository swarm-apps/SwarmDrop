use crate::AppResult;
use crate::device::DeviceFilter;
use crate::events::{DevicesChanged, PairedDeviceAdded};
use crate::network::NetManagerState;
use serde::Serialize;
use swarmdrop_core::device::{DeviceReceivePolicy, DeviceTrustLevel, OsInfo, PairedDeviceInfo};
use swarmdrop_core::protocol::{PairingMethod, PairingResponse};
use swarmdrop_invite::{PairInvite, TransportPolicy};
use swarmdrop_net::{Addr, NodeId, SecretKey};
use tauri::{AppHandle, Manager as _, State};
use tauri_specta::Event as _;

use crate::AppError;

/// 把前端传来的 base58 字符串解析为 [`NodeId`]，失败归一化为 identity 错误。
fn parse_peer_id(peer_id: &str) -> AppResult<NodeId> {
    peer_id
        .parse()
        .map_err(|e| AppError::identity(format!("invalid peer_id: {e}")))
}

/// 邀请串解码后的展示投影（用于配对确认卡；不含 capability 等敏感字段）。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PairInvitePreview {
    /// 发起方 NodeId（base58）。
    pub peer_id: String,
    pub display_name: String,
    pub display_platform: String,
    /// 过期时刻（Unix 秒）——前端与当前时间比对判断是否已过期。
    pub expires_at: i64,
    /// LocalOnly 策略（仅局域网）。
    pub local_only: bool,
}

/// 生成邀请串的二维码 SVG（三端统一编码规范：大写 alphanumeric + ECL::M + quiet zone，
/// 见 `swarmdrop_invite::qr`）。前端 `dangerouslySetInnerHTML` 塞入白卡。
#[tauri::command]
#[specta::specta]
pub fn invite_qr_svg(invite: String) -> AppResult<String> {
    swarmdrop_invite::invite_qr_svg(&invite)
        .map_err(|e| AppError::identity(format!("二维码生成失败: {e}")))
}

/// 解码并验签邀请串，返回对端展示信息（**不发起配对、不消费**）。
///
/// 供受邀方在扫码/粘贴/剪贴板感知后先展示确认卡；篡改/伪造的邀请在此即被验签拒绝。
#[tauri::command]
#[specta::specta]
pub fn decode_pair_invite(invite: String) -> AppResult<PairInvitePreview> {
    let inv =
        PairInvite::decode(&invite).map_err(|e| AppError::identity(format!("邀请无效: {e}")))?;
    Ok(PairInvitePreview {
        peer_id: inv.inviter.id.to_string(),
        display_name: inv.display_name,
        display_platform: inv.display_platform,
        expires_at: inv.expires_at as i64,
        local_only: matches!(inv.transport_policy, TransportPolicy::LocalOnly),
    })
}

/// 生成一次性签名邀请串（供二维码/链接分享）。
///
/// `local_only=true` 走 LocalOnly 策略（受邀方只用私网地址、禁公网 fallback）。
/// 邀请自包含地址提示，不经 DHT——旧 6 位分享码机制已废弃。
#[tauri::command]
#[specta::specta]
pub async fn generate_pair_invite(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    local_only: Option<bool>,
) -> AppResult<String> {
    let secret = app
        .try_state::<SecretKey>()
        .ok_or_else(|| AppError::identity("设备身份未初始化"))?
        .inner()
        .clone();
    let os_info = OsInfo::default();
    let policy = if local_only.unwrap_or(false) {
        TransportPolicy::LocalOnly
    } else {
        TransportPolicy::Auto
    };
    with_manager!(net, |m| AppResult::Ok(
        m.pairing().encode_invite(&secret, policy, &os_info)
    ))
}

/// 用邀请串发起配对（受邀方）：解码验签 → 连接发起方 → 出示凭证。
///
/// 配对成功后自动加入已配对设备并 emit `paired-device-added`。
#[tauri::command]
#[specta::specta]
pub async fn consume_pair_invite(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    invite: String,
) -> AppResult<PairingResponse> {
    let (response, paired_info) =
        with_manager!(net, |m| m.pairing().pair_with_invite(&invite).await)?;

    if let Some(info) = paired_info {
        persist_paired_device(&app, info.clone()).await?;
        let _ = PairedDeviceAdded(info).emit(&app);
        publish_devices_changed(&app, &net).await;
    }

    Ok(response)
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
