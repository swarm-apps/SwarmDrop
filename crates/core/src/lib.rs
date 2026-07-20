//! SwarmDrop 平台无关核心 crate。
//!
//! 这里会承载桌面端和 React Native 移动端共享的身份、网络、配对、
//! 传输和持久化逻辑。当前先建立 crate 边界，后续任务会逐步迁移模块。

pub mod device_manager;
pub mod event_adapter;
pub mod host;
pub mod identity;
pub mod infra;
pub mod network;
pub mod pairing;
pub mod presence;
pub mod protocol;
pub mod runtime;

// 迁出的下层 crate 以别名 re-export，保持 `crate::transfer::` / `crate::device::` /
// `crate::error::` 路径不变，宿主接线与既有调用点零改动（依赖倒置见各端口 trait）。
pub use swarmdrop_host::{AppError, AppResult, device, error};
pub use swarmdrop_transfer as transfer;

/// 当前 core crate 的版本号。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
