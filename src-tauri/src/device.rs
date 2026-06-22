//! `crate::device` 命名空间 alias
//!
//! 把 [`swarmdrop_core::device`] 和 [`swarmdrop_core::device_manager`] 两个 sub-mod
//! 合并到 `crate::device`，使 `crate::device::DeviceFilter` /
//! `crate::device::PairedDeviceInfo` 等路径在 src-tauri 内部仍然可用，无需改动
//! 原引用点。

pub use swarmdrop_core::device::*;
pub use swarmdrop_core::device_manager::*;
