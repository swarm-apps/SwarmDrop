//! 入站传输可信设备策略评估。

use serde::{Deserialize, Serialize};

use crate::device::{DeviceTrustLevel, PairedDeviceInfo};
use crate::host::CoreSaveLocation;
use crate::protocol::FileInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ReceivePolicyAction {
    AutoAccept,
    RequireConfirmation,
    Reject,
}

impl ReceivePolicyAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoAccept => "auto_accept",
            Self::RequireConfirmation => "require_confirmation",
            Self::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ReceivePolicyDecision {
    pub action: ReceivePolicyAction,
    pub reason: String,
    pub save_location: Option<CoreSaveLocation>,
}

impl ReceivePolicyDecision {
    fn auto_accept(reason: impl Into<String>, path: String) -> Self {
        Self {
            action: ReceivePolicyAction::AutoAccept,
            reason: reason.into(),
            save_location: Some(CoreSaveLocation::Path { path }),
        }
    }

    fn require_confirmation(reason: impl Into<String>) -> Self {
        Self {
            action: ReceivePolicyAction::RequireConfirmation,
            reason: reason.into(),
            save_location: None,
        }
    }

    fn reject(reason: impl Into<String>) -> Self {
        Self {
            action: ReceivePolicyAction::Reject,
            reason: reason.into(),
            save_location: None,
        }
    }

    pub fn action_name(&self) -> &'static str {
        self.action.as_str()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceivePolicyContext<'a> {
    pub device: Option<&'a PairedDeviceInfo>,
    pub files: &'a [FileInfo],
    pub total_size: u64,
    pub via_relay: bool,
    pub now_ms: i64,
}

pub fn evaluate_receive_policy(ctx: ReceivePolicyContext<'_>) -> ReceivePolicyDecision {
    let Some(device) = ctx.device else {
        return ReceivePolicyDecision::reject("设备未配对");
    };

    if device.trust_level == DeviceTrustLevel::Blocked {
        return ReceivePolicyDecision::reject("设备已被阻止");
    }

    let policy = &device.receive_policy;

    if let Some(expires_at) = policy.expires_at
        && expires_at <= ctx.now_ms
    {
        return ReceivePolicyDecision::reject("临时设备授权已过期");
    }

    if let Some(max_bytes) = policy.max_transfer_bytes
        && ctx.total_size > max_bytes
    {
        return ReceivePolicyDecision::reject("传输大小超过设备接收策略限制");
    }

    if !policy.allow_directories && ctx.files.iter().any(is_nested_path) {
        return ReceivePolicyDecision::reject("该设备策略不允许自动接收文件夹");
    }

    if !device.trust_confirmed {
        return ReceivePolicyDecision::require_confirmation("设备信任策略需要确认");
    }

    if policy.require_confirmation || !policy.auto_accept {
        return ReceivePolicyDecision::require_confirmation("设备接收策略要求手动确认");
    }

    if ctx.via_relay && !policy.allow_relay_auto_accept {
        return ReceivePolicyDecision::require_confirmation("当前通过中继连接，需手动确认");
    }

    let Some(path) = policy.default_save_location.clone() else {
        return ReceivePolicyDecision::require_confirmation("未配置自动接收保存位置");
    };

    ReceivePolicyDecision::auto_accept("可信设备策略自动接收", path)
}

fn is_nested_path(file: &FileInfo) -> bool {
    file.relative_path.contains('/') || file.relative_path.contains('\\')
}

#[cfg(test)]
mod tests {
    use swarm_p2p_core::libp2p::{PeerId, identity::Keypair};

    use super::{ReceivePolicyAction, ReceivePolicyContext, evaluate_receive_policy};
    use crate::device::{DeviceTrustLevel, OsInfo, PairedDeviceInfo};
    use crate::protocol::FileInfo;

    fn file(relative_path: &str, size: u64) -> FileInfo {
        FileInfo {
            file_id: 1,
            name: relative_path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(relative_path)
                .to_string(),
            relative_path: relative_path.to_string(),
            size,
            checksum: "checksum".to_string(),
        }
    }

    fn device(level: DeviceTrustLevel) -> PairedDeviceInfo {
        let keypair = Keypair::generate_ed25519();
        let mut device = PairedDeviceInfo::new(
            PeerId::from_public_key(&keypair.public()),
            OsInfo {
                name: None,
                hostname: "test".to_string(),
                os: "test".to_string(),
                platform: "test".to_string(),
                arch: "test".to_string(),
                capabilities: Vec::new(),
            },
            1,
        );
        device.apply_trust_level_defaults(level);
        device
    }

    #[test]
    fn collaborator_requires_confirmation() {
        let device = device(DeviceTrustLevel::Collaborator);
        let files = vec![file("a.txt", 1)];
        let decision = evaluate_receive_policy(ReceivePolicyContext {
            device: Some(&device),
            files: &files,
            total_size: 1,
            via_relay: false,
            now_ms: 1,
        });

        assert_eq!(decision.action, ReceivePolicyAction::RequireConfirmation);
    }

    #[test]
    fn owned_auto_accepts_when_save_location_is_configured() {
        let mut device = device(DeviceTrustLevel::Owned);
        device.receive_policy.default_save_location = Some("/tmp/swarmdrop".to_string());
        let files = vec![file("a.txt", 1)];
        let decision = evaluate_receive_policy(ReceivePolicyContext {
            device: Some(&device),
            files: &files,
            total_size: 1,
            via_relay: false,
            now_ms: 1,
        });

        assert_eq!(decision.action, ReceivePolicyAction::AutoAccept);
        assert!(decision.save_location.is_some());
    }

    #[test]
    fn blocked_device_is_rejected() {
        let device = device(DeviceTrustLevel::Blocked);
        let files = vec![file("a.txt", 1)];
        let decision = evaluate_receive_policy(ReceivePolicyContext {
            device: Some(&device),
            files: &files,
            total_size: 1,
            via_relay: false,
            now_ms: 1,
        });

        assert_eq!(decision.action, ReceivePolicyAction::Reject);
    }
}
