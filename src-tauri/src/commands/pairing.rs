use crate::AppResult;
use crate::device::DeviceFilter;
use crate::events::DevicesChanged;
use crate::host::event_bus::TauriEventBus;
use crate::network::NetManagerState;
use serde::Serialize;
use swarmdrop_core::device::{DeviceReceivePolicy, DeviceTrustLevel, PairedDeviceInfo};
use swarmdrop_core::pairing::PairedDeviceCommit;
use swarmdrop_core::protocol::{PairingMethod, PairingResponse};
use swarmdrop_invite::{PairInvite, TransportPolicy};
use swarmdrop_net::{Addr, NodeId, SecretKey};
use tauri::{AppHandle, Manager as _, State};
use tauri_specta::Event as _;

use crate::AppError;

/// 把前端传来的 base58 字符串解析为 [`NodeId`]。
fn parse_peer_id(peer_id: &str) -> AppResult<NodeId> {
    peer_id
        .parse()
        .map_err(|e| AppError::invalid_argument(format!("invalid peer_id: {e}")))
}

/// 一次配对尝试的结果。
///
/// `persisted` 与 [`revoke_pair_invite_by_id`] 的返回值同构：**本次运行内已生效，但重启后
/// 会变回去**。这类「一半成功」不能压成 `Err` —— 配对达成时对端已经把本机加进它的列表，
/// 本机再报失败只会让两台设备对同一件事的认知永久分叉。UI 该说的是「这台设备重启后会丢，
/// 建议重新配对」，不是「配对失败」。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PairingOutcome {
    pub response: PairingResponse,
    /// 设备是否已落盘。**仅当 `response` 为成功时有意义**，其余情况恒为 `true`。
    pub persisted: bool,
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

/// 生成 canonical 邀请链接的二维码 SVG（三端统一编码规范：原样编码 + 最优分段 + ECL::M
/// + quiet zone，见 `swarmdrop_invite::qr`）。前端 `dangerouslySetInnerHTML` 塞入白卡。
#[tauri::command]
#[specta::specta]
pub fn invite_qr_svg(invite: String) -> AppResult<String> {
    swarmdrop_invite::invite_qr_svg(&invite)
        .map_err(|e| AppError::invalid_argument(format!("二维码生成失败: {e}")))
}

/// 解码并验签邀请串，返回对端展示信息（**不发起配对、不消费**）。
///
/// 供受邀方在扫码/粘贴/剪贴板感知后先展示确认卡；篡改/伪造的邀请在此即被验签拒绝。
#[tauri::command]
#[specta::specta]
pub fn decode_pair_invite(invite: String) -> AppResult<PairInvitePreview> {
    // **分类成 InvalidCode，不要包成 Identity。** `kind` 是前端本地化的判别码
    // （见 `src/lib/errors.ts`）：包成 Identity 会让用户看到「设备身份初始化失败」——
    // 一条与「链接不对」毫无关系的提示。技术细节只进日志，不进 UI。
    let inv = PairInvite::decode(&invite).map_err(|e| {
        tracing::debug!("decode_pair_invite 失败: {e}");
        AppError::Core(swarmdrop_core::AppError::InvalidCode)
    })?;
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
    // 身份「还没就绪」不是「读写身份失败」——用户的正确动作是重启应用，不是去查钥匙串。
    let secret = app
        .try_state::<SecretKey>()
        .ok_or_else(AppError::identity_not_ready)?
        .inner()
        .clone();
    let policy = if local_only.unwrap_or(false) {
        TransportPolicy::LocalOnly
    } else {
        TransportPolicy::Auto
    };
    // display 名取 `PairingManager` 持有的本机 OsInfo（组合根从 DeviceConfig 端口装配），
    // 此处不再另传一份 `OsInfo::default()` —— 那正是邀请卡上恒显示占位主机名的成因。
    with_manager!(net, |m| AppResult::Ok(
        m.pairing().encode_invite(&secret, policy).await
    ))
}

/// 撤销本机发出的邀请（重新生成覆盖旧串、用户放弃、关闭邀请界面）。
///
/// 幂等：不认识的串直接 no-op（详见 `PairingManager::revoke_invite`），所以前端可以
/// fire-and-forget。节点未启动时同样无事可做——注册表随节点一起没了（重启后由
/// `load_invites` 从库里读回，见 `invite-persistence`）。
#[tauri::command]
#[specta::specta]
pub async fn revoke_pair_invite(
    net: State<'_, NetManagerState>,
    invite: String,
) -> AppResult<bool> {
    with_manager!(net, |m| AppResult::Ok(
        m.pairing().revoke_invite(&invite).await
    ))
}

/// 「已发出的邀请」列表条目。
///
/// **没有邀请串本身** —— capability 明文不落盘也不出注册表（invite-persistence design D4），
/// 所以重启后拼不回原始链接。UI 只能显示元数据 + 提供撤销；想再分享就生成一条新的。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PairInviteListItem {
    /// `sha256(capability)` 的 hex —— 撤销时回传，UI 当不透明 ID 用。
    pub id: String,
    /// 创建时刻（Unix 秒）。
    pub created_at: i64,
    /// 过期时刻（Unix 秒）。
    pub expires_at: i64,
    /// 已被对方消费（仍在列表里显示到过期，让用户知道它被用过）。
    pub consumed: bool,
}

/// 列出本机未过期的已发出邀请（最近生成的在前）。
///
/// TTL 24h 之后「我现在有几条邀请在外面飘」不再是个可以忽略的问题 —— 这个列表加上
/// [`revoke_pair_invite_by_id`] 是那段窗口的可见性与控制手段，不是可选装饰。
#[tauri::command]
#[specta::specta]
pub async fn list_pair_invites(
    net: State<'_, NetManagerState>,
) -> AppResult<Vec<PairInviteListItem>> {
    with_manager!(net, |m| AppResult::Ok(
        m.pairing()
            .list_invites()
            .into_iter()
            .map(|summary| PairInviteListItem {
                id: hex_lower(&summary.capability_hash),
                created_at: summary.created_at as i64,
                expires_at: summary.expires_at as i64,
                consumed: summary.consumed,
            })
            .collect()
    ))
}

/// 按列表条目的 `id`（capability 哈希 hex）撤销 —— 列表里没有原始邀请串。
///
/// **返回是否已落盘**。`false` 意味着撤销在本次运行内生效了，但重启后那条邀请会复活
/// （写穿失败，库里仍是生成时写下的 pending）—— UI 必须把这件事告诉用户，
/// 否则他会以为已经撤销干净了。
#[tauri::command]
#[specta::specta]
pub async fn revoke_pair_invite_by_id(
    net: State<'_, NetManagerState>,
    id: String,
) -> AppResult<bool> {
    let hash = parse_hex32(&id)?;
    with_manager!(net, |m| AppResult::Ok(
        m.pairing().revoke_invite_by_hash(hash).await
    ))
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn parse_hex32(text: &str) -> AppResult<[u8; 32]> {
    let invalid = || AppError::invalid_argument("邀请标识格式非法");
    if text.len() != 64 {
        return Err(invalid());
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        // **用 `get` 而不是 `&text[a..b]`。** `text.len()` 是**字节**数，64 字节的串完全
        // 可能含多字节字符（如 `"aé" + 61 个 ASCII`），那时按 2 字节切片会落在字符中间
        // 直接 panic —— 而这个入参来自前端 IPC，是用户可控的。`get` 在非字符边界返回
        // `None`，退化成一次干净的「格式非法」。另外三处同款解码（`crates/web` ×2、
        // mobile-core）本来就是 `get`，这里对齐。
        let pair = text.get(index * 2..index * 2 + 2).ok_or_else(invalid)?;
        *slot = u8::from_str_radix(pair, 16).map_err(|_| invalid())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse_hex32;

    /// 回归锚点：64 **字节**但含多字节字符的输入，必须是「格式非法」而不是 panic。
    ///
    /// 这条红了说明有人把 `text.get(..)` 换回了 `&text[..]`。入口是
    /// `revoke_pair_invite_by_id` 这个 IPC 命令，字符串由前端给。
    #[test]
    fn rejects_multibyte_input_without_panicking() {
        let text = format!("a\u{e9}{}", "0".repeat(61));
        assert_eq!(text.len(), 64, "构造的必须是 64 字节");
        assert!(parse_hex32(&text).is_err());
    }

    #[test]
    fn round_trips_valid_hex() {
        let hex = "ab".repeat(32);
        assert_eq!(parse_hex32(&hex).unwrap(), [0xab; 32]);
        assert!(parse_hex32("short").is_err());
        assert!(parse_hex32(&"z".repeat(64)).is_err());
    }
}

/// 用邀请串发起配对（受邀方）：解码验签 → 连接发起方 → 出示凭证。
///
/// 配对成功后由 core 落盘并 emit `paired-device-added`。
///
/// 返回 [`PairingOutcome`]：`response` 是对端的答复，`persisted` 为 `false` 时表示
/// **配对成功了但这条记录没写进钥匙串** —— 本次运行内可用，重启后这台设备会从列表消失
/// （对端仍记着）。UI 必须如实告知，不能当成普通成功。
#[tauri::command]
#[specta::specta]
pub async fn consume_pair_invite(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    invite: String,
) -> AppResult<PairingOutcome> {
    let (response, paired_info) =
        with_manager!(net, |m| m.pairing().pair_with_invite(&invite).await)?;

    let persisted = finish_pairing(&app, &net, paired_info.as_ref()).await;

    Ok(PairingOutcome {
        response,
        persisted,
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
) -> AppResult<PairingOutcome> {
    let peer_id = parse_peer_id(&peer_id)?;
    let addrs = addrs
        .map(|list| {
            list.into_iter()
                .map(|s| s.parse::<Addr>())
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|e| AppError::invalid_argument(format!("invalid multiaddr: {e}")))?;

    let (response, paired_info) = with_manager!(net, |m| m
        .pairing()
        .request_pairing(peer_id, method, addrs)
        .await)?;

    let persisted = finish_pairing(&app, &net, paired_info.as_ref()).await;

    Ok(PairingOutcome {
        response,
        persisted,
    })
}

/// 取消与指定设备的配对。
///
/// 「节点在不在跑」这条分支由 core 的 [`swarmdrop_core::paired_devices::unpair`] 吸收：节点在跑就走
/// `PairingManager::unpair`（持久化 → 共享内存表 → 事件，**fail-closed**，不会再出现
/// 「本次运行解除了、重启又复活」）；没跑则只删持久化并由 core 补发
/// `PairedDeviceRemoved`（那条路径上 `PairingManager` 根本没起）。
/// 桌面此前在这里手工补那条事件、移动端忘了补 —— 同一段分支写两遍必然漂。
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
    let store = crate::host::paired_device_store(&app)?;
    let events = app.state::<TauriEventBus>();

    let guard = net.lock().await;
    swarmdrop_core::paired_devices::unpair(&peer_id, &*store, events.inner(), guard.as_ref())
        .await?;
    Ok(())
}

/// 某信任级别的默认接收策略。
///
/// **纯派生，不取任何 State**——它在节点没起来时也该能用（信任策略对话框可以先开着）。
///
/// 存在的全部理由是**不让前端再抄一份那张表**。此前 `trust-policy-dialog.tsx` 里有一份
/// `defaultPolicyForTrust`、移动端 `device-trust.ts` 里另有一份，两份还长出了不同的
/// 「切级别时保留哪些字段」规则，而内核那一份一个都不保留——同一个产品动作三种行为。
/// 现在规则只在 [`DeviceReceivePolicy::for_trust_level`] 一处。
///
/// `previous` 传该设备**当前**的策略；用户显式设过的保存位置与代收授权会被带过去
/// （`blocked` 除外）。
#[tauri::command]
#[specta::specta]
pub fn default_receive_policy(
    trust_level: DeviceTrustLevel,
    previous: Option<DeviceReceivePolicy>,
) -> DeviceReceivePolicy {
    DeviceReceivePolicy::for_trust_level(trust_level, previous.as_ref())
}

/// 更新已配对设备的可信策略。
///
/// 落盘与「节点在跑时把新值推进共享内存表」都在 core 的
/// [`swarmdrop_core::paired_devices::set_receive_policy`]（否则「策略已保存、本次运行仍按旧策略裁决入站
/// offer」）。存在性检查也只在那一处 —— 它找不到时已经返回 `Err`，命令层再 `find` 一遍
/// 是走不到的死分支。
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
    let store = crate::host::paired_device_store(&app)?;

    let guard = net.lock().await;
    let updated = swarmdrop_core::paired_devices::set_receive_policy(
        &peer_id,
        trust_level,
        receive_policy,
        &*store,
        guard.as_ref(),
    )
    .await?;
    drop(guard);

    publish_devices_changed(&app, &net).await;
    Ok(updated)
}

/// 处理收到的配对请求（接受/拒绝）。
///
/// 接受后由 core 落盘并 emit `paired-device-added` 事件通知前端。
///
/// **返回是否已落盘**（响应本身是入参，不必回传）：`false` = 配对成功但记录没写进钥匙串，
/// 重启后这台设备会不见（对端仍记着）。语义与 [`PairingOutcome::persisted`] 同。
#[tauri::command]
#[specta::specta]
pub async fn respond_pairing_request(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    pending_id: u64,
    method: PairingMethod,
    response: PairingResponse,
) -> AppResult<bool> {
    // 新内核里配对方式已随入站请求缓存在 core 的 pending 表，respond 无需回传；
    // 保留 `method` 参数仅为 IPC 签名稳定（避免前端 bindings 变更）。
    let _ = method;
    let paired_info = with_manager!(net, |m| {
        m.pairing()
            .respond_pairing_request(pending_id, response)
            .await
    })?;

    // 返回是否已落盘（响应本身是入参，不必回传）。语义见 [`PairingOutcome::persisted`]。
    Ok(finish_pairing(&app, &net, paired_info.as_ref()).await)
}

/// 配对达成后宿主侧还剩的唯一一件事：广播设备列表变化。
///
/// **落盘与 `PairedDeviceAdded` 事件都不在这里了** —— 它们由 core 的
/// [`PairingManager::commit_paired_device`](swarmdrop_core::pairing::PairingManager) 一并完成
/// （事件经 `host::event_bus` 转成 tauri typed event）。此前这段编排在本文件里逐字重复三遍、
/// 在 Web 里两遍、移动端又是另一种时机与失败语义，同一个产品动作长出了三种行为。
///
/// 返回**是否已落盘**（含「没配成时报 `true`」那条约定，判据在
/// [`swarmdrop_core::pairing::persisted_or_absent`] —— 三端共用，别在这里重写）。
async fn finish_pairing(
    app: &AppHandle,
    net: &State<'_, NetManagerState>,
    commit: Option<&PairedDeviceCommit>,
) -> bool {
    if commit.is_some() {
        publish_devices_changed(app, net).await;
    }
    swarmdrop_core::pairing::persisted_or_absent(commit)
}

async fn publish_devices_changed(app: &AppHandle, net: &State<'_, NetManagerState>) {
    let guard = net.lock().await;
    let Some(manager) = guard.as_ref() else {
        return;
    };
    let devices = manager.devices().get_devices(DeviceFilter::All);
    let _ = DevicesChanged(devices).emit(app);
}
