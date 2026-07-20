//! NetManager 注入的传输运行时端口。
//!
//! 消费方 `NetManager` 在 `swarmdrop-core`（`NetManager<TTransfer>` 泛型约束此 trait，
//! shutdown 时驱动清理任务）；实现方 `TransferManager` 在本 crate。端口定义在消费侧
//! 依赖的下层（transfer），使 transfer 无需反依赖 core。

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

/// NetManager 注入的传输运行时。
pub trait TransferRuntime: Send + Sync + 'static {
    fn spawn_cleanup_task(transfer: &Arc<Self>, cancel_token: CancellationToken);
}

impl TransferRuntime for () {
    fn spawn_cleanup_task(_transfer: &Arc<Self>, _cancel_token: CancellationToken) {}
}
