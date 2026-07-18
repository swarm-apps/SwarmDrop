//! 跨 wasm 边界的 JS 可见类型（`WebTransferEvent` / [`OfferJson`]）。
//!
//! 本模块**不受 `wasm_browser` 门控**（纯 serde + transfer 类型，无 wasm 依赖）——native
//! 也可编，specta 导出 test（`tests/specta_export.rs`，生成 `static/types/bindings.ts`）
//! 在 native target 上注册这些类型。TS 形状与运行期 serde 序列化（serde_wasm_bindgen /
//! serde_json）逐字段一致。

use serde::Serialize;
use swarmdrop_transfer::events::TransferEvent;
use swarmdrop_transfer::incoming::TransferOfferEvent;
use swarmdrop_transfer::progress::{
    PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent, TransferDbErrorEvent,
    TransferFailedEvent, TransferPausedEvent, TransferProgressEvent, TransferRejectedEvent,
    TransferResumedEvent,
};
use swarmdrop_transfer::protocol::FileInfo;
use swarmdrop_transfer::store::TransferProjection;

/// `TransferEvent` 的可序列化镜像（1:1 变体，字段与 payload 同名）。
///
/// `TransferEvent` 本身未 derive `Serialize`（transfer 不改）——与桌面把它映射进
/// `CoreEvent` 的适配范式一致。`events()` 的 ReadableStream 逐条产出本类型的序列化对象。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebTransferEvent {
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
    pub fn type_name(&self) -> &'static str {
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

/// 挂起 offer 的 JS 投影（`pending_offers()` 返回 `OfferJson[]`）。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OfferJson {
    pub session_id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub total_size: u64,
    pub files: Vec<FileInfo>,
}

/// 连接路径类别（[`swarmdrop_net_base::PathKind`] 的 JS 投影，TS 侧是字符串联合）。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum PathKindJson {
    Local,
    Direct,
    Relayed,
}

impl From<swarmdrop_net_base::PathKind> for PathKindJson {
    fn from(p: swarmdrop_net_base::PathKind) -> Self {
        match p {
            swarmdrop_net_base::PathKind::Local => Self::Local,
            swarmdrop_net_base::PathKind::Direct => Self::Direct,
            swarmdrop_net_base::PathKind::Relayed => Self::Relayed,
        }
    }
}

/// `connect()` 的结构化返回。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionJson {
    pub path: PathKindJson,
    pub addr: String,
}

/// `lookup_share_code()` 的结构化返回（`NodeAddr` 的 JS 投影）。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NodeAddrJson {
    pub id: String,
    pub addrs: Vec<String>,
}

/// Web 壳对外错误。`kind` 供 JS 分支，`message` 供展示。
///
/// wasm-bindgen 方法 reject 的错误值就是本类型的序列化对象（`{ kind, message }`）——
/// **不拍成字符串**（字符串丢了机器可判别的 kind）。JsValue 转换在 `error.rs`。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WebError {
    /// 身份 / 密钥错误。
    Identity { message: String },
    /// 网络 / 连接 / DHT 错误。
    Network { message: String },
    /// 传输错误。
    Transfer { message: String },
    /// 入参非法（地址格式、缺 `/p2p/` 等）。
    InvalidInput { message: String },
    /// 分享码不存在 / 已过期。
    NotFound { message: String },
    /// 存储（OPFS / localStorage）错误。
    Storage { message: String },
}

impl WebError {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            message: message.into(),
        }
    }
}
