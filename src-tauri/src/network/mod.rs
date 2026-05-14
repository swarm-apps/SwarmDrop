//! 网络模块
//!
//! 管理 P2P 节点的启动/关闭、事件循环和运行时网络状态。
//! `NetManager` 是 [`swarmdrop_core::network::NetManager`] 的类型别名，
//! 配合本 crate 的 [`TransferManager`](crate::transfer::offer::TransferManager) 作为 runtime。

mod event_loop;

use std::sync::Arc;

use swarmdrop_core::network::manager::TransferRuntime;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::transfer::offer::TransferManager;

pub use event_loop::spawn_event_loop;
pub use swarmdrop_core::network::NetworkStatus;

pub type NetManager = swarmdrop_core::network::NetManager<TransferManager>;
pub type SharedNetRefs = swarmdrop_core::network::SharedNetRefs<TransferManager>;

impl TransferRuntime for TransferManager {
    fn spawn_cleanup_task(transfer: &Arc<Self>, cancel_token: CancellationToken) {
        transfer.spawn_cleanup_task(cancel_token);
    }
}

/// Tauri 状态中的网络管理器容器。
pub type NetManagerState = Mutex<Option<NetManager>>;
