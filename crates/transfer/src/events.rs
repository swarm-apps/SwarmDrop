//! 传输事件端口——依赖倒置的事件发射面。
//!
//! transfer 不认识 core 的 `CoreEvent`/`EventBus`（后者聚合了 network/pairing 域事件，
//! 且经 `CoreEvent` 反向引用 transfer wire 类型，下沉会成环）。transfer 只发射自己域内
//! 的 [`TransferEvent`]；core 的适配器把它映射进 `CoreEvent` 再走真正的 `EventBus`。

use async_trait::async_trait;

use crate::AppResult;
use crate::inbox::{InboxItemAddedEvent, InboxItemArchivedEvent, InboxItemRemovedEvent};
use crate::incoming::TransferOfferEvent;
use crate::progress::{
    FilePublishEvent, PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent,
    TransferDbErrorEvent, TransferFailedEvent, TransferPausedEvent, TransferProgressEvent,
    TransferRejectedEvent, TransferResumedEvent,
};
use crate::store::TransferProjection;
use crate::text_delivery::TextDeliveryAttention;

/// transfer 域事件（变体名与 payload 与 `CoreEvent` 的 transfer 变体一一对应，
/// core 的适配器做 1:1 映射）。
#[derive(Debug, Clone)]
pub enum TransferEvent {
    /// 文本投递的用户注意力信号；不携带正文。
    TextDeliveryAttention {
        attention: TextDeliveryAttention,
    },
    TransferOfferReceived {
        offer: TransferOfferEvent,
    },
    TransferProgress {
        event: TransferProgressEvent,
    },
    TransferAccepted {
        event: TransferAcceptedEvent,
    },
    TransferRejected {
        event: TransferRejectedEvent,
    },
    TransferCompleted {
        event: TransferCompleteEvent,
    },
    TransferFailed {
        event: TransferFailedEvent,
    },
    TransferPaused {
        event: TransferPausedEvent,
    },
    TransferResumed {
        event: TransferResumedEvent,
    },
    TransferDbError {
        event: TransferDbErrorEvent,
    },
    TransferProjection {
        projection: TransferProjection,
    },
    PrepareProgress {
        event: PrepareProgressEvent,
    },
    /// 单个文件正在从暂存位置发布到用户可见位置（收齐即发布，一个会话里会发生多次）。
    FilePublish {
        event: FilePublishEvent,
    },

    /// 收件箱多了一条。
    ///
    /// ⚠️ **宿主订阅这条，不要自己从 [`Self::TransferCompleted`] 推导。** 推导要依赖
    /// 「先建条目、再发完成事件」这条顺序，而它只以行内注释存在；调换它——例如为了让
    /// 完成事件更快到达 UI——会让所有推导方同时开始拿不到条目，且各自看起来都像自己的
    /// 竞态缺陷。本仓在补这条事件之前已经有三份推导、两份带缺陷、一份根本没接
    /// （spec: `inbox-domain-events`）。
    ///
    /// **只在接收方向发**：发送完成不产生收件箱条目。
    InboxItemAdded {
        event: InboxItemAddedEvent,
    },
    /// 收件箱条目的归档状态变了（两个方向共用）。
    InboxItemArchived {
        event: InboxItemArchivedEvent,
    },
    /// 收件箱条目被删除。
    InboxItemRemoved {
        event: InboxItemRemovedEvent,
    },
}

/// transfer 事件发射端口。core 侧适配器实现，把 [`TransferEvent`] 转 `CoreEvent`。
#[async_trait]
pub trait TransferEventSink: Send + Sync {
    async fn emit(&self, event: TransferEvent) -> AppResult<()>;
}
