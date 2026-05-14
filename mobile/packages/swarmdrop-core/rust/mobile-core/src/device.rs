//! 设备列表 —— discovered + paired 的统一视图,带连接状态/延迟/NAT 类型。

use swarmdrop_core::device::{ConnectionType, Device, DeviceStatus};
use swarmdrop_core::device_manager::DeviceFilter;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileDevice {
    pub peer_id: String,
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
}
