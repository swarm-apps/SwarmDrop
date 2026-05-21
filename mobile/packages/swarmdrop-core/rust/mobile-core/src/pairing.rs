//! 配对 —— 6 位 share code → DHT lookup → request_pairing → upsert keychain。
//!
//! 流程(主动发起方):
//! 1. 对端调 `generate_pairing_code` → 6 位码上链 DHT
//! 2. 本端调 `lookup_device_by_code(code)` → 拿对端 PeerId + OsInfo
//! 3. 本端调 `request_pairing(peer_id, Some(code), addrs)` → handshake
//! 4. Success 后 `PairingCompleted` 事件触发,paired_devices 写入 keychain

use swarm_p2p_core::libp2p::PeerId;
use swarmdrop_core::pairing::code::ShareCodeRecord;
use swarmdrop_core::protocol::{PairingMethod, PairingRefuseReason, PairingResponse};

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::{parse_multiaddrs, parse_peer_id};

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePairingCode {
    pub code: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileRemoteDeviceInfo {
    pub peer_id: String,
    /// 对端用户起的设备名；缺省时 UI 回退到 hostname。
    pub name: Option<String>,
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub arch: String,
    pub listen_addrs: Vec<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl MobileRemoteDeviceInfo {
    fn from_record(peer_id: PeerId, record: ShareCodeRecord) -> Self {
        Self {
            peer_id: peer_id.to_string(),
            name: record.os_info.name,
            hostname: record.os_info.hostname,
            os: record.os_info.os,
            platform: record.os_info.platform,
            arch: record.os_info.arch,
            listen_addrs: record
                .listen_addrs
                .into_iter()
                .map(|addr| addr.to_string())
                .collect(),
            // ShareCodeRecord.created_at / expires_at 仍是 i64 秒（DHT line format），
            // 直接透传；前端 pairing-code-store 也按秒计算（× 1000 转毫秒）。
            created_at: record.created_at,
            expires_at: record.expires_at,
        }
    }
}

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
    pub async fn generate_pairing_code(
        &self,
        expires_in_secs: u64,
    ) -> FfiResult<MobilePairingCode> {
        let pairing = self.pairing_manager().await?;
        let code = pairing
            .generate_code(expires_in_secs)
            .await
            .map_err(FfiError::from)?;
        Ok(MobilePairingCode {
            code: code.code,
            // PairingCodeInfo.created_at / expires_at 是 chrono::DateTime<Utc>
            // (desktop 8d298e5)；FFI 边界保持 i64 秒，与 ShareCodeRecord / 前端约定一致。
            created_at: code.created_at.timestamp(),
            expires_at: code.expires_at.timestamp(),
        })
    }

    pub async fn lookup_device_by_code(&self, code: String) -> FfiResult<MobileRemoteDeviceInfo> {
        let pairing = self.pairing_manager().await?;
        let (peer_id, record) = pairing.get_device_info(&code).await.map_err(FfiError::from)?;
        Ok(MobileRemoteDeviceInfo::from_record(peer_id, record))
    }

    pub async fn request_pairing(
        &self,
        peer_id: String,
        code: Option<String>,
        addrs: Vec<String>,
    ) -> FfiResult<MobilePairingResult> {
        let pairing = self.pairing_manager().await?;
        let peer_id = parse_peer_id(&peer_id)?;
        let addrs = parse_multiaddrs(addrs)?;
        let method = code
            .map(|code| PairingMethod::Code { code })
            .unwrap_or(PairingMethod::Direct);
        let (response, paired) = pairing
            .request_pairing(peer_id, method, Some(addrs))
            .await
            .map_err(FfiError::from)?;
        if let Some(info) = paired {
            swarmdrop_core::identity::upsert_paired_device(self.keychain(), info)
                .await
                .map_err(FfiError::from)?;
        }
        Ok(pairing_result(response))
    }

    pub async fn respond_pairing_request(
        &self,
        pending_id: u64,
        code: Option<String>,
        accept: bool,
    ) -> FfiResult<()> {
        let pairing = self.pairing_manager().await?;
        let method = code
            .map(|code| PairingMethod::Code { code })
            .unwrap_or(PairingMethod::Direct);
        let response = if accept {
            PairingResponse::Success
        } else {
            PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            }
        };

        if let Some(info) = pairing
            .handle_pairing_request(pending_id, &method, response)
            .await
            .map_err(FfiError::from)?
        {
            swarmdrop_core::identity::upsert_paired_device(self.keychain(), info)
                .await
                .map_err(FfiError::from)?;
        }

        Ok(())
    }
}
