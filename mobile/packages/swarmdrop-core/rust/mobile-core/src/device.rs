//! 设备列表 —— discovered + paired 的统一视图,带连接状态/延迟/NAT 类型。

use swarmdrop_core::device::{
    ConnectionType, Device, DeviceReceivePolicy, DeviceStatus, DeviceTrustLevel, PairedDeviceInfo,
    ReceiveSaveBehavior,
};
use swarmdrop_core::device_manager::DeviceFilter;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::parse_peer_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileDeviceTrustLevel {
    Owned,
    Collaborator,
    Temporary,
    Blocked,
}

impl From<DeviceTrustLevel> for MobileDeviceTrustLevel {
    fn from(level: DeviceTrustLevel) -> Self {
        match level {
            DeviceTrustLevel::Owned => Self::Owned,
            DeviceTrustLevel::Collaborator => Self::Collaborator,
            DeviceTrustLevel::Temporary => Self::Temporary,
            DeviceTrustLevel::Blocked => Self::Blocked,
        }
    }
}

impl From<MobileDeviceTrustLevel> for DeviceTrustLevel {
    fn from(level: MobileDeviceTrustLevel) -> Self {
        match level {
            MobileDeviceTrustLevel::Owned => Self::Owned,
            MobileDeviceTrustLevel::Collaborator => Self::Collaborator,
            MobileDeviceTrustLevel::Temporary => Self::Temporary,
            MobileDeviceTrustLevel::Blocked => Self::Blocked,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileReceiveSaveBehavior {
    InboxAndDefaultSaveLocation,
}

impl From<ReceiveSaveBehavior> for MobileReceiveSaveBehavior {
    fn from(behavior: ReceiveSaveBehavior) -> Self {
        match behavior {
            ReceiveSaveBehavior::InboxAndDefaultSaveLocation => Self::InboxAndDefaultSaveLocation,
        }
    }
}

impl From<MobileReceiveSaveBehavior> for ReceiveSaveBehavior {
    fn from(behavior: MobileReceiveSaveBehavior) -> Self {
        match behavior {
            MobileReceiveSaveBehavior::InboxAndDefaultSaveLocation => {
                Self::InboxAndDefaultSaveLocation
            }
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileDeviceReceivePolicy {
    pub auto_accept: bool,
    pub require_confirmation: bool,
    pub max_transfer_bytes: Option<u64>,
    pub allow_directories: bool,
    pub allow_relay_auto_accept: bool,
    pub save_behavior: MobileReceiveSaveBehavior,
    pub default_save_location: Option<String>,
    pub allow_mcp_send_to_device: bool,
    pub expires_at: Option<i64>,
}

impl From<DeviceReceivePolicy> for MobileDeviceReceivePolicy {
    fn from(policy: DeviceReceivePolicy) -> Self {
        // 穷尽解构：上游加字段时这里会编译失败，强制同步。
        let DeviceReceivePolicy {
            auto_accept,
            require_confirmation,
            max_transfer_bytes,
            allow_directories,
            allow_relay_auto_accept,
            save_behavior,
            default_save_location,
            allow_mcp_send_to_device,
            // 移动端暂不管理 MCP「接收方接受」策略(桌面侧功能),不镜像到 RN。
            allow_mcp_accept_from_device: _,
            expires_at,
        } = policy;
        Self {
            auto_accept,
            require_confirmation,
            max_transfer_bytes,
            allow_directories,
            allow_relay_auto_accept,
            save_behavior: save_behavior.into(),
            default_save_location,
            allow_mcp_send_to_device,
            expires_at,
        }
    }
}

impl From<MobileDeviceReceivePolicy> for DeviceReceivePolicy {
    fn from(policy: MobileDeviceReceivePolicy) -> Self {
        Self {
            auto_accept: policy.auto_accept,
            require_confirmation: policy.require_confirmation,
            max_transfer_bytes: policy.max_transfer_bytes,
            allow_directories: policy.allow_directories,
            allow_relay_auto_accept: policy.allow_relay_auto_accept,
            save_behavior: policy.save_behavior.into(),
            default_save_location: policy.default_save_location,
            allow_mcp_send_to_device: policy.allow_mcp_send_to_device,
            // 移动端不携带该字段 → 回写时 fail-closed 为 false(安全默认)。
            // 若后续要在移动端管理 MCP 接受策略,需镜像字段并重生成 bindings。
            allow_mcp_accept_from_device: false,
            expires_at: policy.expires_at,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileDevice {
    pub peer_id: String,
    /// 用户起的设备名；缺省时 UI 回退到 hostname。
    pub name: Option<String>,
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub arch: String,
    pub status: String,
    pub connection: Option<String>,
    pub latency_ms: Option<u64>,
    pub is_paired: bool,
    pub trust_level: Option<MobileDeviceTrustLevel>,
    pub receive_policy: Option<MobileDeviceReceivePolicy>,
    pub trust_confirmed: Option<bool>,
}

impl From<Device> for MobileDevice {
    fn from(device: Device) -> Self {
        // 穷尽解构：上游加字段时这里会编译失败，强制同步。
        let Device {
            peer_id,
            os_info,
            status,
            connection,
            latency,
            is_paired,
            trust_level,
            receive_policy,
            trust_confirmed,
        } = device;
        Self {
            peer_id: peer_id.to_string(),
            name: os_info.name,
            hostname: os_info.hostname,
            os: os_info.os,
            platform: os_info.platform,
            arch: os_info.arch,
            status: match status {
                DeviceStatus::Online => "online".to_string(),
                DeviceStatus::Offline => "offline".to_string(),
            },
            connection: connection.map(|connection| match connection {
                ConnectionType::Lan => "lan".to_string(),
                ConnectionType::Dcutr => "dcutr".to_string(),
                ConnectionType::Relay => "relay".to_string(),
            }),
            latency_ms: latency,
            is_paired,
            trust_level: trust_level.map(Into::into),
            receive_policy: receive_policy.map(Into::into),
            trust_confirmed,
        }
    }
}

impl From<PairedDeviceInfo> for MobileDevice {
    fn from(info: PairedDeviceInfo) -> Self {
        // 穷尽解构：上游加字段时这里会编译失败，强制同步。
        let PairedDeviceInfo {
            peer_id,
            os_info,
            paired_at: _,
            trust_level,
            receive_policy,
            trust_confirmed,
        } = info;
        Self {
            peer_id: peer_id.to_string(),
            name: os_info.name,
            hostname: os_info.hostname,
            os: os_info.os,
            platform: os_info.platform,
            arch: os_info.arch,
            status: "offline".to_string(),
            connection: None,
            latency_ms: None,
            is_paired: true,
            trust_level: Some(trust_level.into()),
            receive_policy: Some(receive_policy.into()),
            trust_confirmed: Some(trust_confirmed),
        }
    }
}

pub(crate) fn parse_device_filter(value: &str) -> FfiResult<DeviceFilter> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "all" => Ok(DeviceFilter::All),
        "connected" => Ok(DeviceFilter::Connected),
        "paired" => Ok(DeviceFilter::Paired),
        other => Err(FfiError::Identity(format!(
            "invalid device filter: {other}"
        ))),
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn list_devices(&self, filter: String) -> FfiResult<Vec<MobileDevice>> {
        let filter = parse_device_filter(&filter)?;
        let guard = self.net_manager_guard().await;
        let manager = guard.as_ref().ok_or(FfiError::NodeNotStarted)?;
        Ok(manager
            .devices()
            .get_devices(filter)
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 直接读 keychain 里的已配对设备清单 —— 不依赖 NetManager,
    /// 节点未启动时也可调,用于 UI 离线兜底视图。
    pub async fn list_paired_devices(&self) -> FfiResult<Vec<MobileDevice>> {
        let devices = swarmdrop_core::identity::load_paired_devices(self.keychain())
            .await
            .map_err(FfiError::from)?;
        Ok(devices.into_iter().map(Into::into).collect())
    }

    pub async fn update_paired_device_policy(
        &self,
        peer_id: String,
        trust_level: MobileDeviceTrustLevel,
        receive_policy: Option<MobileDeviceReceivePolicy>,
    ) -> FfiResult<MobileDevice> {
        let peer_id = parse_peer_id(&peer_id)?;
        let devices = swarmdrop_core::identity::update_paired_device_policy(
            self.keychain(),
            &peer_id,
            trust_level.into(),
            receive_policy.map(Into::into),
        )
        .await
        .map_err(FfiError::from)?;
        let updated = devices
            .into_iter()
            .find(|device| device.peer_id == peer_id)
            .ok_or_else(|| FfiError::Identity("paired device not found".into()))?;

        let guard = self.net_manager_guard().await;
        if let Some(manager) = guard.as_ref() {
            manager.pairing().add_paired_device(updated.clone());
        }

        Ok(updated.into())
    }

    pub async fn remove_paired_device(&self, peer_id: String) -> FfiResult<Vec<MobileDevice>> {
        let peer_id = parse_peer_id(&peer_id)?;
        let devices = swarmdrop_core::identity::remove_paired_device(self.keychain(), &peer_id)
            .await
            .map_err(FfiError::from)?;

        let guard = self.net_manager_guard().await;
        if let Some(manager) = guard.as_ref() {
            manager.pairing().remove_paired_device(&peer_id);
        }

        Ok(devices.into_iter().map(Into::into).collect())
    }
}
