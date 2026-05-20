//! 设备列表 —— discovered + paired 的统一视图,带连接状态/延迟/NAT 类型。

use swarmdrop_core::device::{ConnectionType, Device, DeviceStatus, PairedDeviceInfo};
use swarmdrop_core::device_manager::DeviceFilter;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};

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
}

impl From<Device> for MobileDevice {
    fn from(device: Device) -> Self {
        Self {
            peer_id: device.peer_id.to_string(),
            name: device.os_info.name,
            hostname: device.os_info.hostname,
            os: device.os_info.os,
            platform: device.os_info.platform,
            arch: device.os_info.arch,
            status: match device.status {
                DeviceStatus::Online => "online".to_string(),
                DeviceStatus::Offline => "offline".to_string(),
            },
            connection: device.connection.map(|connection| match connection {
                ConnectionType::Lan => "lan".to_string(),
                ConnectionType::Dcutr => "dcutr".to_string(),
                ConnectionType::Relay => "relay".to_string(),
            }),
            latency_ms: device.latency,
            is_paired: device.is_paired,
        }
    }
}

impl From<PairedDeviceInfo> for MobileDevice {
    fn from(info: PairedDeviceInfo) -> Self {
        Self {
            peer_id: info.peer_id.to_string(),
            name: info.os_info.name,
            hostname: info.os_info.hostname,
            os: info.os_info.os,
            platform: info.os_info.platform,
            arch: info.os_info.arch,
            status: "offline".to_string(),
            connection: None,
            latency_ms: None,
            is_paired: true,
        }
    }
}

pub(crate) fn parse_device_filter(value: &str) -> FfiResult<DeviceFilter> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "all" => Ok(DeviceFilter::All),
        "connected" => Ok(DeviceFilter::Connected),
        "paired" => Ok(DeviceFilter::Paired),
        other => Err(FfiError::Identity(format!("invalid device filter: {other}"))),
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
}
