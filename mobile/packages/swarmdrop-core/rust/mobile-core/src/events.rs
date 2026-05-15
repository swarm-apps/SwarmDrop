//! 事件总线 —— core `CoreEvent` 通过 `ForeignEventBus` callback 回 JS。
//!
//! 关键决策:
//! - 镜像 enum `MobileCoreEvent`(不在共享 crate 上加 uniffi derive)
//! - 事件单向:Rust → JS;JS 没法直接 emit `CoreEvent`
//! - `spawn_event_loop` 在 node 启动时挂载,主循环退出时一并退出

use std::sync::Arc;

use async_trait::async_trait;
use swarm_p2p_core::EventReceiver;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::network::SharedNetRefs;
use swarmdrop_core::protocol::{AppRequest, PairingMethod};
use swarmdrop_core::AppResult;

use crate::network::MobileNetworkStatus;
use crate::transfer::MobileTransferOffer;

#[derive(Debug, Clone, uniffi::Enum)]
pub enum MobileCoreEvent {
    NetworkStatusChanged {
        status: MobileNetworkStatus,
    },
    DevicesChanged,
    PairingRequestReceived {
        peer_id: String,
        pending_id: u64,
        code: Option<String>,
    },
    PairingCompleted {
        peer_id: String,
    },
    TransferProgress {
        session_id: String,
        progress: f32,
    },
    TransferOfferReceived {
        offer: MobileTransferOffer,
    },
    TransferCompleted {
        session_id: String,
    },
    TransferFailed {
        session_id: String,
        error: String,
    },
    TransferPaused {
        session_id: String,
    },
    TransferDbError {
        session_id: String,
        message: String,
    },
    Error {
        message: String,
    },
}

#[uniffi::export(with_foreign)]
pub trait ForeignEventBus: Send + Sync {
    fn emit(&self, event: MobileCoreEvent);
}

pub(crate) struct MobileEventBusAdapter {
    foreign: Arc<dyn ForeignEventBus>,
}

impl MobileEventBusAdapter {
    pub(crate) fn new(foreign: Arc<dyn ForeignEventBus>) -> Self {
        Self { foreign }
    }
}

#[async_trait]
impl EventBus for MobileEventBusAdapter {
    async fn publish(&self, event: CoreEvent) -> AppResult<()> {
        if let Some(mobile_event) = map_event(event) {
            self.foreign.emit(mobile_event);
        }
        Ok(())
    }
}

fn map_event(event: CoreEvent) -> Option<MobileCoreEvent> {
    match event {
        CoreEvent::NetworkStatusChanged { status } => Some(MobileCoreEvent::NetworkStatusChanged {
            status: status.into(),
        }),
        CoreEvent::DevicesChanged { .. } => Some(MobileCoreEvent::DevicesChanged),
        CoreEvent::PairingRequestReceived {
            peer_id,
            pending_id,
            request,
        } => {
            let code = match request.method {
                PairingMethod::Code { code } => Some(code),
                PairingMethod::Direct => None,
            };
            Some(MobileCoreEvent::PairingRequestReceived {
                peer_id: peer_id.to_string(),
                pending_id,
                code,
            })
        }
        CoreEvent::PairingCompleted { peer_id } => {
            Some(MobileCoreEvent::PairingCompleted { peer_id })
        }
        CoreEvent::TransferProgress { event } => Some(MobileCoreEvent::TransferProgress {
            session_id: event.session_id.to_string(),
            // 取百分比近似（移动端 MobileCoreEvent::TransferProgress payload 还未扩展）
            progress: if event.total_bytes > 0 {
                event.transferred_bytes as f32 / event.total_bytes as f32
            } else {
                0.0
            },
        }),
        CoreEvent::TransferOfferReceived { offer } => Some(MobileCoreEvent::TransferOfferReceived {
            offer: offer.into(),
        }),
        CoreEvent::TransferCompleted { event } => Some(MobileCoreEvent::TransferCompleted {
            session_id: event.session_id.to_string(),
        }),
        CoreEvent::TransferFailed { event } => Some(MobileCoreEvent::TransferFailed {
            session_id: event.session_id.to_string(),
            error: event.error,
        }),
        CoreEvent::TransferPaused { event } => Some(MobileCoreEvent::TransferPaused {
            session_id: event.session_id.to_string(),
        }),
        CoreEvent::TransferDbError { event } => Some(MobileCoreEvent::TransferDbError {
            session_id: event.session_id.to_string(),
            message: event.message,
        }),
        CoreEvent::Error { message } => Some(MobileCoreEvent::Error { message }),
        // 新增的事件（PairedDeviceAdded / TransferAccepted / TransferRejected /
        // TransferResumed / PrepareProgress）当前 MobileCoreEvent 还没镜像，
        // 等下一轮 mobile-core 接线时补充。
        _ => None,
    }
}

pub(crate) fn spawn_event_loop(
    mut receiver: EventReceiver<AppRequest>,
    shared: SharedNetRefs<()>,
    event_bus: Arc<MobileEventBusAdapter>,
) {
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if let Err(error) = swarmdrop_core::network::event_loop::handle_core_node_event(
                &shared,
                &event,
                event_bus.as_ref(),
            )
            .await
            {
                let _ = event_bus
                    .publish(CoreEvent::Error {
                        message: error.to_string(),
                    })
                    .await;
            }
        }
    });
}
