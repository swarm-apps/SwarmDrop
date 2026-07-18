//! 网络模块（薄壳）
//!
//! 类型别名 + `spawn_event_loop` 转发；事件循环与 router 保活装配全部在
//! [`swarmdrop_core::network::event_loop`] 中。

use swarmdrop_core::transfer::manager::TransferManager;
use tokio::sync::Mutex;

pub use swarmdrop_core::network::NetworkStatus;
pub use swarmdrop_core::network::event_loop::spawn_event_loop;

pub type NetManager = swarmdrop_core::network::NetManager<TransferManager>;

/// Tauri 状态中的网络管理器容器。
pub type NetManagerState = Mutex<Option<NetManager>>;
