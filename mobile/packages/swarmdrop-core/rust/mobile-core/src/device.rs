//! 设备列表 —— discovered + paired 的统一视图,带连接状态/延迟/NAT 类型。
//!
//! 本机设备名的读写（`get_device_name` / `rename_device`）也挂在这里:它是「设备」
//! 域里唯一与列表无关的一对,落盘交给 [`crate::device_config`] 的端口实现。

use swarmdrop_core::device::{
    ConnectionType, Device, DeviceName, DeviceReceivePolicy, DeviceStatus, DeviceTrustLevel,
    PairedDeviceInfo, ReceiveSaveBehavior,
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
    /// 本机设备名 —— 用户设过的那个；未设过返回 `None`，UI 回退系统 hostname。
    ///
    /// 事实源在 Rust 侧的 `device_config.json`，JS 的 `preferences-store.deviceName`
    /// 只是显示镜像。
    pub async fn get_device_name(&self) -> FfiResult<Option<String>> {
        Ok(self
            .device_config()
            .load_device_name()
            .await
            .map(DeviceName::into_string))
    }

    /// 改本机设备名：写盘 + 让已连接的对端立刻看到 + 通知本端各处 UI。
    ///
    /// `None`（或归一化后为空）表示清空，回退系统 hostname。归一化只有
    /// [`DeviceName::parse`] 一个入口（trim、剥控制字符与 `;`、按 char 截到 40）；
    /// UI 的 `maxLength` 是提前拦，不是唯一防线。
    ///
    /// **返回归一化后的名字**（`None` = 已清空）。它可能与传进来的不同，而 JS 侧的
    /// 显示镜像要写的正是这个值 —— 不返回的话调用方只能紧接着再 `getDeviceName()`
    /// 回读一次，白跨一趟 FFI 去问一个这里本来就握在手里的结果。
    ///
    /// 编排在 core 的 [`rename_device`](swarmdrop_core::device_name::rename_device)：
    /// 节点在跑就顺带更新内存态并向已连接对端推 identify，节点没跑（onboarding、
    /// 或设置页早于 `start_node`）就只落盘。**两条分支都不由本壳判断** —— 本壳只把
    /// `Option<NetManager>` 原样交出去。
    pub async fn rename_device(&self, name: Option<String>) -> FfiResult<Option<String>> {
        let name = name.as_deref().and_then(DeviceName::parse);
        let guard = self.net_manager_guard().await;
        swarmdrop_core::device_name::rename_device(
            name.clone(),
            self.device_config(),
            self.event_bus(),
            guard.as_ref(),
        )
        .await
        .map_err(FfiError::from)?;
        Ok(name.map(DeviceName::into_string))
    }

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

    /// 直接读持久化的已配对设备清单 —— 不依赖 NetManager,
    /// 节点未启动时也可调,用于 UI 离线兜底视图。
    ///
    /// 整份读没有算法可言，直接用端口；`paired_devices` 那层只在「读-改-写」时才有存在
    /// 意义（那才是三端会各写一遍并漂开的部分）。
    pub async fn list_paired_devices(&self) -> FfiResult<Vec<MobileDevice>> {
        let devices = self
            .paired_device_store()
            .load_paired_devices()
            .await
            .map_err(FfiError::from)?;
        Ok(devices.into_iter().map(Into::into).collect())
    }

    /// 改一台已配对设备的信任级别与收件策略。
    ///
    /// 编排在 core 的
    /// [`set_receive_policy`](swarmdrop_core::paired_devices::set_receive_policy)：落盘之外
    /// 还要在节点在跑时把新值推进共享内存表（`swarmdrop_transfer::policy` 裁决入站 offer
    /// 读的是那份，不推就是「策略已保存、本次运行仍按旧策略放行」）。**分支不由本壳判断。**
    ///
    /// 存在性检查也只在 core 那一处 —— 它找不到该 peer 时直接返回 `Err`，本壳不必再
    /// `find` 一遍。
    pub async fn update_paired_device_policy(
        &self,
        peer_id: String,
        trust_level: MobileDeviceTrustLevel,
        receive_policy: Option<MobileDeviceReceivePolicy>,
    ) -> FfiResult<MobileDevice> {
        let peer_id = parse_peer_id(&peer_id)?;
        let guard = self.net_manager_guard().await;
        let updated = swarmdrop_core::paired_devices::set_receive_policy(
            &peer_id,
            trust_level.into(),
            receive_policy.map(Into::into),
            self.paired_device_store(),
            guard.as_ref(),
        )
        .await
        .map_err(FfiError::from)?;

        Ok(updated.into())
    }

    /// 解除配对。
    ///
    /// 编排在 core 的 [`unpair`](swarmdrop_core::paired_devices::unpair)：节点在跑时走
    /// [`PairingManager::unpair`](swarmdrop_core::pairing::PairingManager::unpair)（原子地
    /// 持久化 → 删共享内存表 → 发事件，删内存表是 presence 撤销的唯一开关，只删持久化
    /// 的话保活与重探会一直跑到进程退出）；节点没跑时只改持久化，**并由 core 补发**
    /// `PairedDeviceRemoved`。
    ///
    /// 本壳此前自己写这个 match，且漏了「没跑时补发事件」那半 —— 而
    /// [`MobileCoreEvent::PairedDeviceRemoved`](crate::events::MobileCoreEvent) 只由该事件
    /// 触发，于是节点没起时解除配对，RN 侧收不到任何移除事件，列表里的设备不会消失。
    /// 现在两条分支的事件语义由 core 统一保证。
    pub async fn remove_paired_device(&self, peer_id: String) -> FfiResult<Vec<MobileDevice>> {
        let peer_id = parse_peer_id(&peer_id)?;
        let guard = self.net_manager_guard().await;
        let devices = swarmdrop_core::paired_devices::unpair(
            &peer_id,
            self.paired_device_store(),
            self.event_bus(),
            guard.as_ref(),
        )
        .await
        .map_err(FfiError::from)?;

        Ok(devices.into_iter().map(Into::into).collect())
    }
}
