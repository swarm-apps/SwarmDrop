//! 文本投递领域的垂直边界。
//!
//! 这里聚合文本专用的模型、wire、待确认状态机与服务；文件会话协议与通用存储端口仍留在
//! 各自领域，避免文本能力重新把依赖铺回 crate 根目录。

mod attention;
mod model;
mod pending;
mod service;
mod wire;

pub use attention::{TextDeliveryAttention, TextDeliveryAttentionKind};
pub use model::{
    MAX_TEXT_DELIVERY_BYTES, TEXT_DELIVERY_CONFIRMATION_TIMEOUT, TEXT_DELIVERY_PREVIEW_BYTES,
    TextDeliveryRecord, decode_text_body, text_preview, validate_text_body,
};
pub use pending::PendingTextDeliverySummary;
pub use service::TextDeliveryService;
pub use wire::{
    TEXT_DELIVERY, TEXT_DELIVERY_PROTOCOL, TextDeliveryRejectReason, TextDeliveryRequest,
    TextDeliveryResponse,
};
