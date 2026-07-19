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
use swarmdrop_core::protocol::{PairingRefuseReason, PairingResponse};
use swarmdrop_invite::TransportPolicy;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePairingResult {
    pub accepted: bool,
    pub reason: Option<String>,
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
