//! 跨 wasm 边界的 JS 可见类型（`WebTransferEvent` / [`OfferJson`]）。
//!
//! 本模块**不受 `wasm_browser` 门控**（纯 serde + transfer 类型，无 wasm 依赖）——native
//! 也可编，specta 导出 test（`tests/specta_export.rs`，生成 `bindings/bindings.ts`）
//! 在 native target 上注册这些类型。TS 形状与运行期 serde 序列化（serde_wasm_bindgen /
//! serde_json）逐字段一致。

use serde::Serialize;
// `paired_devices()` 的 JS 返回类型：直接复用桌面同款读模型（已 Serialize + specta::Type），
// 不再手写一份 Web 专属投影——字段（含在线状态/连接类型）语义与桌面一致，没有理由分叉。
pub use swarmdrop_host::device::Device;
// 收件箱 DTO 同理：领域模型住在 `swarmdrop_transfer::inbox`，三端共用一份形状，
// Web 侧只把它们抬进 JS 可见类型层供 specta 导出，不另造投影。
use swarmdrop_transfer::events::TransferEvent;
pub use swarmdrop_transfer::inbox::{
    InboxHitFile, InboxItemDetail, InboxItemFileEntry, InboxItemSummary, InboxSearchHit,
};
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

/// 挂起入站配对请求的 JS 投影（`pending_pairing_requests()` 返回）。
///
/// browser-as-inviter 用：桌面消费浏览器生成的 invite 后，浏览器（邀请方）本机需弹确认。
/// `pendingId` 以字符串承载核心的 u64 pending id（避免 JS number 精度 + serde_wasm_bindgen
/// 对 u64 产 BigInt 的取回麻烦），`respond_pairing_request` 再 parse 回 u64。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PendingPairingJson {
    pub pending_id: String,
    pub peer_id: String,
    pub device_name: String,
}

/// 一次配对尝试的结果（与桌面 `PairingOutcome` / 移动 `MobilePairingResult` 同构）。
///
/// `persisted` 表达的是一种**不能压成错误的「一半成功」**：走到这一步对端已经收到
/// `Success` 并把本机加进了它的已配对列表，本机此时若报失败，两台设备对同一件事的认知
/// 就永久分叉了。真实后果只有一个 —— 这台设备刷新/重启后会从本机列表消失（对端仍记着），
/// UI 该照这个说。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairingOutcomeJson {
    /// 已配对对端的 NodeId（base58）。
    pub peer_id: String,
    /// 设备是否已落盘。`false` = 刷新页面后这台设备会不见（对端仍记着）。
    pub persisted: bool,
}

/// 「已发出的邀请」列表条目（openspec: invite-persistence）。
///
/// **没有邀请串本身**：capability 明文不落盘也不出注册表，刷新后拼不回原始链接。
/// UI 只显示元数据 + 提供撤销；想再分享就生成一条新的。
///
/// 时间戳用字符串承载 Unix 秒，与 `pendingId` 同一个理由（避免 u64 → BigInt 的取回麻烦）。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InviteListItemJson {
    /// `sha256(capability)` 的 hex —— 撤销时回传，UI 当不透明 ID 用。
    pub id: String,
    pub created_at: String,
    pub expires_at: String,
    /// 已被对方消费（仍显示到过期，让用户知道它被用过）。
    pub consumed: bool,
}

/// 邀请串解码后的展示投影（配对确认卡用）。
///
/// **不含 capability**：那是 128bit bearer 凭据，明文绝不出 wasm 边界——确认卡只需要
/// 「这是谁、还有效多久、是不是仅局域网」，多给一个字段就多一条泄漏路径。
///
/// **TTL 由调用方按 `expiresAt` 判**，这里不放 `expired: bool`：确认卡会在屏幕上停留几十秒，
/// 而布尔在序列化那一刻就开始变旧。权威判定本来也在发起端的 `InviteRegistry`，
/// 解码侧的预检只为 UX（见 `PairInvite::decode` 的文档）。
///
/// `expiresAt` 用字符串承载 Unix 秒，与 [`InviteListItemJson`] 同一个理由。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairInvitePreviewJson {
    /// 发起方 NodeId（base58）。取自签名覆盖范围内的 `inviter_id`，伪造不了——
    /// 自我过滤 / 已配对过滤都该以它为判据。
    pub peer_id: String,
    pub display_name: String,
    pub display_platform: String,
    pub expires_at: String,
    /// LocalOnly 策略（受邀方只用私网地址、禁公网 fallback）。
    pub local_only: bool,
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

/// relay reservation 状态类别（[`swarmdrop_net::RelayState`] 的 JS 投影，TS 侧字符串联合）。
#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum RelayStateKind {
    Connecting,
    Active,
    Failed,
}

/// 单个 relay 意图的状态快照（`relays_state()` 元素 / `relays_changed()` 流的产出单元）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RelayInfoJson {
    /// relay 节点身份（base58 NodeId）。
    pub id: String,
    pub state: RelayStateKind,
    /// `active` 时为本机经该 relay 的完整可达地址（内核拼装下发），其余为 null。
    pub circuit_addr: Option<String>,
    /// `failed` 时的末次错误描述，其余为 null。重试轮数不下发——那是
    /// 策略层内账（supervisor 唯一持有），诊断走日志。
    pub last_error: Option<String>,
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
    /// 调用被 `AbortSignal` 取消。**abort ≠ 撤回拨号**：Promise 立即 reject 且
    /// 无常驻意图残留，但在途拨号会继续到自然失败（libp2p 无逐次拨号取消面）。
    Aborted { message: String },
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

    pub fn aborted(message: impl Into<String>) -> Self {
        Self::Aborted {
            message: message.into(),
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            message: message.into(),
        }
    }
}
