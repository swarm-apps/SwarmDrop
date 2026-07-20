//! [`TransferEventSink`] → [`CoreEvent`]/[`EventBus`] 适配器（依赖倒置的 core 侧接口）。
//!
//! transfer 域只发射自己的 [`TransferEvent`]（它反向引用 transfer wire 类型，无法与
//! `CoreEvent` 同处一层）；core 在组合根注入本适配器，把 transfer 事件 1:1 映射进聚合
//! `CoreEvent` 再走真正的 [`EventBus`]。

use std::sync::Arc;

use async_trait::async_trait;
use swarmdrop_transfer::events::{TransferEvent, TransferEventSink};

use crate::AppResult;
use crate::host::{CoreEvent, EventBus};

/// 注入 `TransferManager` 的事件端口实现：转 `CoreEvent` 后走宿主 `EventBus`。
pub struct CoreTransferEvents(pub Arc<dyn EventBus>);

#[async_trait]
impl TransferEventSink for CoreTransferEvents {
    async fn emit(&self, event: TransferEvent) -> AppResult<()> {
        self.0.publish(event.into()).await
    }
}

impl From<TransferEvent> for CoreEvent {
    fn from(e: TransferEvent) -> Self {
        match e {
            TransferEvent::TransferOfferReceived { offer } => {
                CoreEvent::TransferOfferReceived { offer }
            }
            TransferEvent::TransferProgress { event } => CoreEvent::TransferProgress { event },
            TransferEvent::TransferAccepted { event } => CoreEvent::TransferAccepted { event },
            TransferEvent::TransferRejected { event } => CoreEvent::TransferRejected { event },
            TransferEvent::TransferCompleted { event } => CoreEvent::TransferCompleted { event },
            TransferEvent::TransferFailed { event } => CoreEvent::TransferFailed { event },
            TransferEvent::TransferPaused { event } => CoreEvent::TransferPaused { event },
            TransferEvent::TransferResumed { event } => CoreEvent::TransferResumed { event },
            TransferEvent::TransferDbError { event } => CoreEvent::TransferDbError { event },
            TransferEvent::TransferProjection { projection } => {
                CoreEvent::TransferProjection { projection }
            }
            TransferEvent::PrepareProgress { event } => CoreEvent::PrepareProgress { event },
        }
    }
}
