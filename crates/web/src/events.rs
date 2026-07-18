//! 传输事件端口的 Web 实现：`TransferEvent` 经无界 channel 出，`events()` 的
//! [`ReadableStream`](wasm_streams::ReadableStream) 单点消费、逐个 serde-wasm-bindgen 序列化。
//!
//! `TransferEvent` 本身未 derive `Serialize`（transfer 不改）——定义 web 侧可序列化镜像
//! [`WebTransferEvent`]（`#[serde(tag="type", rename_all="camelCase")]`，variant 与字段都标），
//! 与桌面把 `TransferEvent` 映射进 `CoreEvent` 的适配范式一致。序列化在消费侧（主线程）做，
//! channel 里只走 `Send` 的 `TransferEvent`（其 DTO 都是普通数据），满足端口的 Send 约束。

use async_trait::async_trait;
use futures::channel::mpsc;
use serde::Serialize;
use swarmdrop_host::AppResult;
use swarmdrop_transfer::events::{TransferEvent, TransferEventSink};
use swarmdrop_transfer::incoming::TransferOfferEvent;
use swarmdrop_transfer::progress::{
    PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent, TransferDbErrorEvent,
    TransferFailedEvent, TransferPausedEvent, TransferProgressEvent, TransferRejectedEvent,
    TransferResumedEvent,
};
use swarmdrop_transfer::store::TransferProjection;
use wasm_bindgen::JsValue;

/// 事件发射端口：把 transfer 域事件塞进无界 channel。
pub struct WebEventSink {
    tx: mpsc::UnboundedSender<TransferEvent>,
}

impl WebEventSink {
    /// 建 sink 与配套 receiver（receiver 交给 `events()` 构 ReadableStream，单点消费）。
    pub fn new() -> (Self, mpsc::UnboundedReceiver<TransferEvent>) {
        let (tx, rx) = mpsc::unbounded();
        (Self { tx }, rx)
    }
}

#[async_trait]
impl TransferEventSink for WebEventSink {
    async fn emit(&self, event: TransferEvent) -> AppResult<()> {
        // 消费方（ReadableStream）已 drop 时静默丢弃，不影响传输本身。
        let _ = self.tx.unbounded_send(event);
        Ok(())
    }
}

/// 把一个 `TransferEvent` 序列化成结构化 JS 对象（消费侧、主线程调用）。
///
/// 序列化失败**不静默吞成 NULL**：显式 `warn` 打出错因（否则「事件没到 JS」极难定位）。
/// 仍返回 NULL（reader 侧对 null 有容错），但根因可见。
pub fn serialize_event(event: TransferEvent) -> JsValue {
    let web = WebTransferEvent::from(event);
    match serde_wasm_bindgen::to_value(&web) {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(event = web.type_name(), "事件序列化失败（返回 NULL）: {e}");
            JsValue::NULL
        }
    }
}

/// `TransferEvent` 的可序列化镜像（1:1 变体，字段与 payload 同名）。
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WebTransferEvent {
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

impl WebTransferEvent {
    /// 变体静态名（诊断日志用，与 `#[serde(rename_all="camelCase")]` 的 tag 对齐）。
    fn type_name(&self) -> &'static str {
        match self {
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
        }
    }
}

impl From<TransferEvent> for WebTransferEvent {
    fn from(e: TransferEvent) -> Self {
        match e {
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
        }
    }
}
