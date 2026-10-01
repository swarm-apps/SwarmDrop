//! 浏览器传输事件的可序列化协议与流出口。
use serde::Serialize;
use swarmdrop_transfer::events::TransferEvent;
use swarmdrop_transfer::inbox::{
    InboxItemAddedEvent, InboxItemArchivedEvent, InboxItemRemovedEvent,
};
use swarmdrop_transfer::incoming::TransferOfferEvent;
use swarmdrop_transfer::progress::{
    FilePublishEvent, PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent,
    TransferDbErrorEvent, TransferFailedEvent, TransferPausedEvent, TransferProgressEvent,
    TransferRejectedEvent, TransferResumedEvent,
};
use swarmdrop_transfer::store::TransferProjection;
use swarmdrop_transfer::text_delivery::TextDeliveryAttention;

/// `TransferEvent` 的可序列化镜像（1:1 变体，字段与 payload 同名）。
///
/// `TransferEvent` 本身未 derive `Serialize`（transfer 不改）——与桌面把它映射进
/// `CoreEvent` 的适配范式一致。`events()` 的 ReadableStream 逐条产出本类型的序列化对象。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebTransferEvent {
    TextDeliveryAttention { attention: TextDeliveryAttention },
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
    FilePublish { event: FilePublishEvent },
    InboxItemAdded { event: InboxItemAddedEvent },
    InboxItemArchived { event: InboxItemArchivedEvent },
    InboxItemRemoved { event: InboxItemRemovedEvent },
}

impl WebTransferEvent {
    /// 变体静态名（诊断日志用，与 `#[serde(rename_all="camelCase")]` 的 tag 对齐）。
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::TextDeliveryAttention { .. } => "textDeliveryAttention",
            Self::TransferOfferReceived { .. } => "transferOfferReceived",
            Self::TransferProgress { .. } => "transferProgress",
            Self::TransferAccepted { .. } => "transferAccepted",
            Self::TransferRejected { .. } => "transferRejected",
            Self::TransferCompleted { .. } => "transferCompleted",
            Self::TransferFailed { .. } => "transferFailed",
            Self::TransferPaused { .. } => "transferPaused",
            Self::TransferResumed { .. } => "transferResumed",
            Self::TransferDbError { .. } => "transferDbError",
            Self::TransferProjection { .. } => "transferProjection",
            Self::PrepareProgress { .. } => "prepareProgress",
            Self::FilePublish { .. } => "filePublish",
            Self::InboxItemAdded { .. } => "inboxItemAdded",
            Self::InboxItemArchived { .. } => "inboxItemArchived",
            Self::InboxItemRemoved { .. } => "inboxItemRemoved",
        }
    }
}

impl From<TransferEvent> for WebTransferEvent {
    fn from(e: TransferEvent) -> Self {
        match e {
            TransferEvent::TextDeliveryAttention { attention } => {
                Self::TextDeliveryAttention { attention }
            }
            TransferEvent::TransferOfferReceived { offer } => Self::TransferOfferReceived { offer },
            TransferEvent::TransferProgress { event } => Self::TransferProgress { event },
            TransferEvent::TransferAccepted { event } => Self::TransferAccepted { event },
            TransferEvent::TransferRejected { event } => Self::TransferRejected { event },
            TransferEvent::TransferCompleted { event } => Self::TransferCompleted { event },
            TransferEvent::TransferFailed { event } => Self::TransferFailed { event },
            TransferEvent::TransferPaused { event } => Self::TransferPaused { event },
            TransferEvent::TransferResumed { event } => Self::TransferResumed { event },
            TransferEvent::TransferDbError { event } => Self::TransferDbError { event },
            TransferEvent::TransferProjection { projection } => {
                Self::TransferProjection { projection }
            }
            TransferEvent::PrepareProgress { event } => Self::PrepareProgress { event },
            TransferEvent::FilePublish { event } => Self::FilePublish { event },
            TransferEvent::InboxItemAdded { event } => Self::InboxItemAdded { event },
            TransferEvent::InboxItemArchived { event } => Self::InboxItemArchived { event },
            TransferEvent::InboxItemRemoved { event } => Self::InboxItemRemoved { event },
        }
    }
}

#[cfg(wasm_browser)]
mod browser;
#[cfg(wasm_browser)]
pub use browser::{WebEventSink, serialize_event};
