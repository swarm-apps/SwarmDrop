use std::sync::Arc;

use swarmdrop_core::network::manager::TransferRuntime;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::transfer::offer::TransferManager;

pub type NetManager = swarmdrop_core::network::NetManager<TransferManager>;
pub type SharedNetRefs = swarmdrop_core::network::SharedNetRefs<TransferManager>;

impl TransferRuntime for TransferManager {
    fn spawn_cleanup_task(transfer: &Arc<Self>, cancel_token: CancellationToken) {
        transfer.spawn_cleanup_task(cancel_token);
    }
}

/// Tauri 状态中的网络管理器容器。
pub type NetManagerState = Mutex<Option<NetManager>>;
