//! 传输事件端口——依赖倒置的事件发射面。
//!
//! transfer 不认识 core 的 `CoreEvent`/`EventBus`（后者聚合了 network/pairing 域事件，
//! 且经 `CoreEvent` 反向引用 transfer wire 类型，下沉会成环）。transfer 只发射自己域内
//! 的 [`TransferEvent`]；core 的适配器把它映射进 `CoreEvent` 再走真正的 `EventBus`。

use async_trait::async_trait;

use crate::AppResult;
use crate::incoming::TransferOfferEvent;
use crate::progress::{
    PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent, TransferDbErrorEvent,
    TransferFailedEvent, TransferPausedEvent, TransferProgressEvent, TransferRejectedEvent,
    TransferResumedEvent,
};
use crate::store::TransferProjection;

/// transfer 域事件（变体名与 payload 与 `CoreEvent` 的 transfer 变体一一对应，
/// core 的适配器做 1:1 映射）。
#[derive(Debug, Clone)]
pub enum TransferEvent {
    TransferOfferReceived { offer: TransferOfferEvent },
    TransferProgress { event: TransferProgressEvent },
    TransferAccepted { event: TransferAcceptedEvent },
    TransferRejected { event: TransferRejectedEvent },
    TransferCompleted { event: TransferCompleteEvent },
    TransferFailed { event: TransferFailedEvent },
    TransferPaused { event: TransferPausedEvent },
    TransferResumed { event: TransferResumedEvent },
    TransferDbError { event: TransferDbErrorEvent },
    TransferProjection { projection: TransferProjection },
    PrepareProgress { event: PrepareProgressEvent },
}

/// transfer 事件发射端口。core 侧适配器实现，把 [`TransferEvent`] 转 `CoreEvent`。
#[async_trait]
pub trait TransferEventSink: Send + Sync {
    async fn emit(&self, event: TransferEvent) -> AppResult<()>;
}
