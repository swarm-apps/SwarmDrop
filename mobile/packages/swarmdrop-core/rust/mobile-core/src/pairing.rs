//! 配对 —— 一次性签名邀请（PairInvite）。6 位分享码 + DHT 已废弃
//! （openspec: pair-invite-protocol）。
//!
//! 流程：
//! 1. 发起方调 `generate_pair_invite(local_only)` → 自包含签名邀请串（二维码/链接分享）
//! 2. 受邀方调 `consume_pair_invite(invite)` → 解码验签 → 连接发起方 → 出示凭证握手
//! 3. Success 后 publish `PairedDeviceAdded`（`MobileEventBusAdapter::publish` 一并
//!    写 keychain + emit 给 JS，见下）
//!
//! 配对成功为什么是 publish 而不是直接写 keychain：`MobileEventBusAdapter::publish`
//! 已经把「写 keychain + emit 给 JS」两件事一起做了，一次 publish 就够；JS 的
//! `pairedDevicesCache` 只在收到事件时刷新。与桌面 `commands/pairing.rs` 对称。

use swarmdrop_core::device::OsInfo;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::protocol::{PairingMethod, PairingRefuseReason, PairingResponse};
use swarmdrop_invite::{PairInvite, TransportPolicy};

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::parse_peer_id;

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePairingResult {
    pub accepted: bool,
    pub reason: Option<String>,
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

fn pairing_result(response: PairingResponse) -> MobilePairingResult {
    match response {
        PairingResponse::Success => MobilePairingResult {
            accepted: true,
            reason: None,
        },
        PairingResponse::Refused { reason } => MobilePairingResult {
            accepted: false,
            reason: Some(format!("{reason:?}")),
        },
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    /// 发起方：生成一次性签名邀请串（供二维码/链接分享）。
    /// `local_only=true` 走 LocalOnly 策略（受邀方只用私网地址、禁公网 fallback）。
    pub async fn generate_pair_invite(&self, local_only: bool) -> FfiResult<String> {
        let secret = self.ensure_keypair().await?;
        let pairing = self.pairing_manager().await?;
        let policy = if local_only {
            TransportPolicy::LocalOnly
        } else {
            TransportPolicy::Auto
        };
        Ok(pairing.encode_invite(&secret, policy, &OsInfo::default()))
    }

    /// 生成邀请串的二维码模块矩阵（RN 按此绘制；三端统一编码规范见 `swarmdrop_invite::qr`）。
    pub fn invite_qr_matrix(&self, invite: String) -> FfiResult<MobileQrMatrix> {
        let matrix = swarmdrop_invite::invite_qr_matrix(&invite)
            .map_err(|e| FfiError::Identity(format!("二维码生成失败: {e}")))?;
        let size = matrix.len() as u32;
        let modules = matrix.into_iter().flatten().collect();
        Ok(MobileQrMatrix { size, modules })
    }

    /// 解码并验签邀请串，返回对端展示信息（**不发起配对**）——扫码/粘贴后先看确认卡。
    pub fn decode_pair_invite(&self, invite: String) -> FfiResult<MobileInvitePreview> {
        let inv = PairInvite::decode(&invite)
            .map_err(|e| FfiError::Identity(format!("邀请无效: {e}")))?;
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
        if let Some(info) = paired {
            self.event_bus_arc()
                .publish(CoreEvent::PairedDeviceAdded { device: info })
                .await
                .map_err(FfiError::from)?;
        }
        Ok(pairing_result(response))
    }

    /// 受邀方：用邀请串发起配对（解码验签 → 连接发起方 → 出示凭证握手）。
    pub async fn consume_pair_invite(&self, invite: String) -> FfiResult<MobilePairingResult> {
        let pairing = self.pairing_manager().await?;
        let (response, paired) = pairing
            .pair_with_invite(&invite)
            .await
            .map_err(FfiError::from)?;
        if let Some(info) = paired {
            self.event_bus_arc()
                .publish(CoreEvent::PairedDeviceAdded { device: info })
                .await
                .map_err(FfiError::from)?;
        }
        Ok(pairing_result(response))
    }

    pub async fn respond_pairing_request(&self, pending_id: u64, accept: bool) -> FfiResult<()> {
        let pairing = self.pairing_manager().await?;
        let response = if accept {
            PairingResponse::Success
        } else {
            PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            }
        };

        if let Some(info) = pairing
            .respond_pairing_request(pending_id, response)
            .await
            .map_err(FfiError::from)?
        {
            self.event_bus_arc()
                .publish(CoreEvent::PairedDeviceAdded { device: info })
                .await
                .map_err(FfiError::from)?;
        }

        Ok(())
    }
}
