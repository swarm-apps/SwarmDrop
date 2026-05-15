//! 网络模块（薄壳）
//!
//! 类型别名 + spawn_event_loop 包装；事件循环逻辑全部在
//! [`swarmdrop_core::network::event_loop`] 中。

use std::sync::Arc;

use swarmdrop_core::host::EventBus;
use swarmdrop_core::transfer::manager::TransferManager;
use tauri::AppHandle;
use tokio::sync::Mutex;

pub use swarmdrop_core::network::NetworkStatus;

pub type NetManager = swarmdrop_core::network::NetManager<TransferManager>;
pub type SharedNetRefs = swarmdrop_core::network::SharedNetRefs<TransferManager>;

/// Tauri 状态中的网络管理器容器。
pub type NetManagerState = Mutex<Option<NetManager>>;

/// 启动事件循环：把 core 的 run_event_loop spawn 到 tokio task。
pub fn spawn_event_loop(
    receiver: swarm_p2p_core::EventReceiver<swarmdrop_core::protocol::AppRequest>,
    app: AppHandle,
    shared: SharedNetRefs,
    event_bus: Arc<dyn EventBus>,
) {
    let notifier: Arc<dyn swarmdrop_core::host::Notifier> =
        Arc::new(crate::host::notifier::DesktopNotifier::new(app));

    tokio::spawn(async move {
        swarmdrop_core::network::event_loop::run_event_loop(
            receiver,
            shared,
            event_bus,
            Some(notifier),
        )
        .await;
    });
}
