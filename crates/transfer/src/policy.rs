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
    /// 宿主**此刻**的默认接收落点。
    ///
    /// 设备策略的 `default_save_location` 为空时用它——那意味着「跟随宿主默认」，而不是
    /// 「没有落点」。宿主此前是在**设置策略时**把当时的落点抄一份进去，于是用户之后换了
    /// 目录、或那个目录被删，自动接收仍然照着旧值写：要么落在用户已经不再期待的地方，
    /// 要么在接受之后才失败——正是「接受前校验」本该消除的那种沉默。
    pub host_default_save_location: Option<&'a str>,
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

    // 按设备的显式覆盖优先；没有覆盖就跟随宿主当下的默认落点。两者都没有才退回手动确认。
    let Some(path) = policy
        .default_save_location
        .clone()
        .or_else(|| ctx.host_default_save_location.map(str::to_owned))
    else {
        return ReceivePolicyDecision::require_confirmation("未配置自动接收保存位置");
    };

    ReceivePolicyDecision::auto_accept("可信设备策略自动接收", path)
}

/// 文本投递复用设备信任、大小和 relay 策略，但不要求文件保存目录。
pub fn evaluate_text_receive_policy(
    device: Option<&PairedDeviceInfo>,
    body_bytes: u64,
    via_relay: bool,
    now_ms: i64,
) -> ReceivePolicyDecision {
    let Some(device) = device else {
        return ReceivePolicyDecision::reject("设备未配对");
    };
    if device.trust_level == DeviceTrustLevel::Blocked {
        return ReceivePolicyDecision::reject("设备已被阻止");
    }
    let policy = &device.receive_policy;
    if policy
        .expires_at
        .is_some_and(|expires_at| expires_at <= now_ms)
        || policy
            .max_transfer_bytes
            .is_some_and(|max| body_bytes > max)
    {
        return ReceivePolicyDecision::reject("设备接收策略拒绝");
    }
    if !device.trust_confirmed
        || policy.require_confirmation
        || !policy.auto_accept
        || (via_relay && !policy.allow_relay_auto_accept)
    {
        return ReceivePolicyDecision::require_confirmation("设备接收策略要求手动确认");
    }
    ReceivePolicyDecision {
        action: ReceivePolicyAction::AutoAccept,
        reason: "可信设备策略自动接收".into(),
        save_location: None,
    }
}

fn is_nested_path(file: &FileInfo) -> bool {
    file.relative_path.contains('/') || file.relative_path.contains('\\')
}

#[cfg(test)]
mod tests {
    use swarmdrop_net::SecretKey;

    use super::{
        ReceivePolicyAction, ReceivePolicyContext, evaluate_receive_policy,
        evaluate_text_receive_policy,
    };
    use crate::device::{DeviceTrustLevel, OsInfo, PairedDeviceInfo};
    use crate::host::CoreSaveLocation;
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
        let mut device = PairedDeviceInfo::new(
            SecretKey::generate().node_id(),
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
            host_default_save_location: None,
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
            host_default_save_location: None,
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
            host_default_save_location: None,
        });

        assert_eq!(decision.action, ReceivePolicyAction::Reject);
    }

    /// 设备策略没有显式落点时跟随宿主当下的默认值。
    ///
    /// 这条替代的是宿主侧「设置策略时把当时的落点抄一份进去」——那份快照会随用户换目录
    /// 而过期，自动接收于是继续往旧目录写（或在接受之后才失败）。跟随宿主意味着落点永远
    /// 是此刻那一个。
    #[test]
    fn auto_accept_falls_back_to_host_default_location() {
        let mut device = device(DeviceTrustLevel::Owned);
        device.receive_policy.default_save_location = None;
        let files = vec![file("a.txt", 1)];
        let decision = evaluate_receive_policy(ReceivePolicyContext {
            device: Some(&device),
            files: &files,
            total_size: 1,
            via_relay: false,
            now_ms: 1,
            host_default_save_location: Some("/tmp/host-default"),
        });

        assert_eq!(decision.action, ReceivePolicyAction::AutoAccept);
        assert_eq!(
            decision.save_location,
            Some(CoreSaveLocation::Path {
                path: "/tmp/host-default".to_string()
            })
        );
    }

    /// 按设备的显式覆盖压过宿主默认——那是用户对这一台设备单独做的决定。
    #[test]
    fn explicit_device_location_wins_over_host_default() {
        let mut device = device(DeviceTrustLevel::Owned);
        device.receive_policy.default_save_location = Some("/tmp/per-device".to_string());
        let files = vec![file("a.txt", 1)];
        let decision = evaluate_receive_policy(ReceivePolicyContext {
            device: Some(&device),
            files: &files,
            total_size: 1,
            via_relay: false,
            now_ms: 1,
            host_default_save_location: Some("/tmp/host-default"),
        });

        assert_eq!(
            decision.save_location,
            Some(CoreSaveLocation::Path {
                path: "/tmp/per-device".to_string()
            })
        );
    }

    /// 两者都没有才退回手动确认——自动接收开着但没有落点可用，不该悄悄收下。
    #[test]
    fn no_location_anywhere_requires_confirmation() {
        let mut device = device(DeviceTrustLevel::Owned);
        device.receive_policy.default_save_location = None;
        let files = vec![file("a.txt", 1)];
        let decision = evaluate_receive_policy(ReceivePolicyContext {
            device: Some(&device),
            files: &files,
            total_size: 1,
            via_relay: false,
            now_ms: 1,
            host_default_save_location: None,
        });

        assert_eq!(decision.action, ReceivePolicyAction::RequireConfirmation);
    }

    #[test]
    fn text_auto_accept_does_not_require_a_file_save_location() {
        let mut device = device(DeviceTrustLevel::Owned);
        device.receive_policy.default_save_location = None;
        let decision = evaluate_text_receive_policy(Some(&device), 64, false, 1);
        assert_eq!(decision.action, ReceivePolicyAction::AutoAccept);
        assert!(decision.save_location.is_none());
    }

    #[test]
    fn text_respects_relay_confirmation_and_size_limit_boundaries() {
        let mut device = device(DeviceTrustLevel::Owned);
        device.receive_policy.max_transfer_bytes = Some(64);
        device.receive_policy.allow_relay_auto_accept = false;
        assert_eq!(
            evaluate_text_receive_policy(Some(&device), 64, false, 1).action,
            ReceivePolicyAction::AutoAccept,
            "边界值本身允许，防止把 > 意外写成 >=",
        );
        assert_eq!(
            evaluate_text_receive_policy(Some(&device), 65, false, 1).action,
            ReceivePolicyAction::Reject,
        );
        assert_eq!(
            evaluate_text_receive_policy(Some(&device), 64, true, 1).action,
            ReceivePolicyAction::RequireConfirmation,
        );
    }
}
