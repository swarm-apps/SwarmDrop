//! SwarmDrop 平台无关核心 crate。
//!
//! 这里会承载桌面端和 React Native 移动端共享的身份、网络、配对、
//! 传输和持久化逻辑。当前先建立 crate 边界，后续任务会逐步迁移模块。

pub mod database;
pub mod device;
pub mod device_manager;
pub mod dht_key;
pub mod error;
pub mod host;
pub mod identity;
pub mod infra;
pub mod network;
pub mod pairing;
pub mod presence;
pub mod protocol;
pub mod runtime;
pub mod transfer;

/// 当前 core crate 的版本号。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use error::{AppError, AppResult};
