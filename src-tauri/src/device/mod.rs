//! 设备模块
//!
//! 管理本机 OS 信息、运行时 peer 发现和已配对设备状态。
//! [`DeviceManager`] 维护 peer 列表并提供统一的设备查询接口。

pub mod manager;
mod utils;

pub use manager::{DeviceFilter, DeviceManager};
pub use swarmdrop_core::device::{
    ConnectionType, Device, DeviceListResult, DeviceStatus, OsInfo, PairedDeviceInfo,
};
