//! 配对 —— 一次性签名邀请（PairInvite）。6 位分享码 + DHT 已废弃
//! （openspec: pair-invite-protocol）。
//!
//! 流程：
//! 1. 发起方调 `generate_pair_invite(local_only)` → 自包含签名邀请串（二维码/链接分享）
//! 2. 受邀方调 `consume_pair_invite(invite)` → 解码验签 → 连接发起方 → 出示凭证握手
//! 3. Success 后由 core 的 `PairingManager::commit_paired_device` 一并完成
//!    「写 keychain + publish `PairedDeviceAdded`」，本壳只把结果转成 FFI 类型
//!
//! **本壳不写 keychain、也不 publish。** 这两件事一度分散在三端各自的 host 层
//! （移动端在 event bus 里回写、桌面在命令层回写），同一个动作因此有了三种失败语义。
//! 现在唯一入口在 core，本壳只负责把 `persisted` 如实带回 JS ——
//! 见 [`MobilePairingResult::persisted`]。

use swarmdrop_core::pairing::PairedDeviceCommit;
use swarmdrop_core::protocol::{PairingMethod, PairingRefuseReason, PairingResponse};
use swarmdrop_invite::{
    PairInvite, TransportPolicy, capability_hash_from_hex, capability_hash_to_hex,
};

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::parse_peer_id;

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePairingResult {
    pub accepted: bool,
    pub reason: Option<String>,
    /// 设备是否已落盘。**仅当 `accepted` 为真时有意义**，其余情况恒为 `true`。
    ///
    /// `false` = 配对成功了、本次运行内这台设备可用，但重启后它会从本机列表消失
    /// （对端仍记着）。不能压成错误：走到这一步对端已经把本机加进它的列表，
    /// 本机报「配对失败」只会让两台设备的认知永久分叉。UI 该说的是「重启后会丢」。
    pub persisted: bool,
}

/// 「已发出的邀请」列表条目（openspec: invite-persistence）。
///
/// **没有邀请串本身**：capability 明文不落盘也不出注册表，重启后拼不回原始链接。
/// UI 只显示元数据 + 提供撤销；想再分享就生成一条新的。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInviteListItem {
    /// `sha256(capability)` 的 hex —— 撤销时回传，UI 当不透明 ID 用。
    pub id: String,
    /// 创建时刻（Unix 秒）。
    pub created_at: i64,
    /// 过期时刻（Unix 秒）。
    pub expires_at: i64,
    /// 已被对方消费（仍显示到过期，让用户知道它被用过）。
    pub consumed: bool,
}

/// 邀请串解码后的展示投影（配对确认卡；不含 capability 等敏感字段）。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInvitePreview {
    pub peer_id: String,
    pub display_name: String,
    pub display_platform: String,
    /// 过期时刻（Unix 秒）——前端与当前时间比对。
    pub expires_at: i64,
    pub local_only: bool,
}

/// 二维码模块矩阵（RN 用 react-native-svg 按此绘 `<Rect>`；已含 quiet zone）。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileQrMatrix {
    /// 边长（模块数）。
    pub size: u32,
    /// 行优先的模块值（`true` = 深模块），长度 = size × size。
    pub modules: Vec<bool>,
}

fn pairing_result(
    response: PairingResponse,
    paired: Option<PairedDeviceCommit>,
) -> MobilePairingResult {
    // 「没配成时报 true」的判据在 core（三端共用），本壳不重写。
    let persisted = swarmdrop_core::pairing::persisted_or_absent(paired.as_ref());
    match response {
        PairingResponse::Success => MobilePairingResult {
            accepted: true,
            reason: None,
            persisted,
        },
        // **稳定的 snake_case 判别码，不是 `{reason:?}`。** Debug 格式化会把
        // `UserRejected` 这种裸 Rust 标识符直接送上 UI —— 既不是中文也不是英文，
        // 是个内部符号。文案由 JS 侧本地化（见 `found-device.tsx`）。
        // 穷尽 match：将来加变体时这里编译失败，比悄悄漏一个好。
        PairingResponse::Refused { reason } => MobilePairingResult {
            accepted: false,
            reason: Some(
                match reason {
                    PairingRefuseReason::UserRejected => "user_rejected",
                }
                .to_owned(),
            ),
            persisted,
        },
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    /// 发起方：生成一次性签名邀请串（供二维码/链接分享）。
    /// `local_only=true` 走 LocalOnly 策略（受邀方只用私网地址、禁公网 fallback）。
    ///
    /// 邀请卡上的名字读 core 持有的本机 `OsInfo`，不由这里传入 —— 此前传的是
    /// `OsInfo::default()`，于是对方看到的永远是 "Device" 这个占位主机名。
    pub async fn generate_pair_invite(&self, local_only: bool) -> FfiResult<String> {
        let secret = self.ensure_keypair().await?;
        let pairing = self.pairing_manager().await?;
        let policy = if local_only {
            TransportPolicy::LocalOnly
        } else {
            TransportPolicy::Auto
        };
        Ok(pairing.encode_invite(&secret, policy).await?)
    }

    /// 撤销本机发出的邀请（重新生成覆盖旧串、用户放弃、关闭邀请界面）。
    ///
    /// 后端幂等——不认识的串 no-op；节点未启动时这里直接 Err，但那种情况下 registry
    /// 本就是空的，调用方 fire-and-forget 即可（详见 `PairingManager::revoke_invite`）。
    /// 返回**是否已落盘**：`false` 时重启后那条邀请会复活，UI 应当提示用户。
    pub async fn revoke_pair_invite(&self, invite: String) -> FfiResult<bool> {
        Ok(self.pairing_manager().await?.revoke_invite(&invite).await)
    }

    /// 本机未过期的已发出邀请（最近生成的在前）。
    ///
    /// TTL 24h + 跨重启存活之后，「我现在有几条邀请在外面飘」需要能看见 —— 这个列表与
    /// [`Self::revoke_pair_invite_by_id`] 是那段窗口的可见性与控制手段。
    pub async fn list_pair_invites(&self) -> FfiResult<Vec<MobileInviteListItem>> {
        Ok(self
            .pairing_manager()
            .await?
            .list_invites()
            .into_iter()
            .map(|summary| MobileInviteListItem {
                id: capability_hash_to_hex(&summary.capability_hash),
                created_at: summary.created_at as i64,
                expires_at: summary.expires_at as i64,
                consumed: summary.consumed,
            })
            .collect())
    }

    /// 按列表条目的 `id`（capability 哈希 hex）撤销 —— 列表里没有原始邀请串。
    pub async fn revoke_pair_invite_by_id(&self, id: String) -> FfiResult<bool> {
        let hash = capability_hash_from_hex(&id)
            .ok_or_else(|| FfiError::InvalidArgument("邀请标识格式非法".to_owned()))?;
        Ok(self
            .pairing_manager()
            .await?
            .revoke_invite_by_hash(hash)
            .await)
    }

    /// 生成邀请串的二维码模块矩阵（RN 按此绘制；三端统一编码规范见 `swarmdrop_invite::qr`）。
    ///
    /// `face_px` 是二维码**实际占据的边长**——RN 端那个 `size` 是白卡外框，要减掉两侧内边距
    /// 再传。它同时是地址提示的预算：放不下时按价值反序回收地址。
    pub fn invite_qr_matrix(&self, invite: String, face_px: u32) -> FfiResult<MobileQrMatrix> {
        let matrix = swarmdrop_invite::invite_qr_matrix(&invite, face_px)
            .map_err(|e| FfiError::InvalidArgument(format!("二维码生成失败: {e}")))?;
        let size = matrix.len() as u32;
        let modules = matrix.into_iter().flatten().collect();
        Ok(MobileQrMatrix { size, modules })
    }

    /// 解码并验签邀请串，返回对端展示信息（**不发起配对**）——扫码/粘贴后先看确认卡。
    pub fn decode_pair_invite(&self, invite: String) -> FfiResult<MobileInvitePreview> {
        // 分类成 InvalidCode（语言中立判别码），文案由 JS 侧本地化。包成
        // `Identity(format!("邀请无效: {e}"))` 会把一条 Rust 中文串直接甩到 UI 上 ——
        // 英文界面下就露馅了。技术细节只进日志。
        let inv = PairInvite::decode(&invite).map_err(|e| {
            tracing::debug!("decode_pair_invite 失败: {e}");
            FfiError::InvalidCode
        })?;
        Ok(MobileInvitePreview {
            peer_id: inv.inviter.id.to_string(),
            display_name: inv.display_name,
            display_platform: inv.display_platform,
            expires_at: inv.expires_at as i64,
            local_only: matches!(inv.transport_policy, TransportPolicy::LocalOnly),
        })
    }

    /// LAN 近场直连配对（同局域网点按；对端 mDNS 校验，与桌面 directPairing 对称）。
    pub async fn pair_direct(&self, peer_id: String) -> FfiResult<MobilePairingResult> {
        let peer_id = parse_peer_id(&peer_id)?;
        let pairing = self.pairing_manager().await?;
        let (response, paired) = pairing
            .request_pairing(peer_id, PairingMethod::Direct, None)
            .await
            .map_err(FfiError::from)?;
        // 落盘与 `PairedDeviceAdded` 事件都已由 core 的 `commit_paired_device` 完成，
        // 这里不再各做一遍（此前移动端在 event bus 里回写、桌面在命令层回写，同一个动作
        // 两种时机、两种失败语义）。
        Ok(pairing_result(response, paired))
    }

    /// 受邀方：用邀请串发起配对（解码验签 → 连接发起方 → 出示凭证握手）。
    pub async fn consume_pair_invite(&self, invite: String) -> FfiResult<MobilePairingResult> {
        let pairing = self.pairing_manager().await?;
        let (response, paired) = pairing
            .pair_with_invite(&invite)
            .await
            .map_err(FfiError::from)?;
        Ok(pairing_result(response, paired))
    }

    /// 响应入站配对请求。**返回是否已落盘**（语义见
    /// [`MobilePairingResult::persisted`]）—— 响应本身是入参，不必回传。
    pub async fn respond_pairing_request(&self, pending_id: u64, accept: bool) -> FfiResult<bool> {
        let pairing = self.pairing_manager().await?;
        let response = if accept {
            PairingResponse::Success
        } else {
            PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            }
        };

        let paired = pairing
            .respond_pairing_request(pending_id, response)
            .await
            .map_err(FfiError::from)?;

        Ok(swarmdrop_core::pairing::persisted_or_absent(
            paired.as_ref(),
        ))
    }
}
