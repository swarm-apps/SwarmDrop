//! 传输事件端口的 Web 实现：`TransferEvent` 经无界 channel 出，`events()` 的
//! [`ReadableStream`](wasm_streams::ReadableStream) 单点消费、逐个 serde-wasm-bindgen 序列化
//! 成 [`WebTransferEvent`]（镜像类型定义在 [`crate::types`]）。序列化在消费侧做，channel 里
//! 只走 `Send` 的 `TransferEvent`（其 DTO 都是普通数据），满足端口的 Send 约束。

use async_trait::async_trait;
use futures::channel::mpsc;
use swarmdrop_host::AppResult;
use swarmdrop_transfer::events::{TransferEvent, TransferEventSink};
use wasm_bindgen::JsValue;

use crate::types::WebTransferEvent;

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
