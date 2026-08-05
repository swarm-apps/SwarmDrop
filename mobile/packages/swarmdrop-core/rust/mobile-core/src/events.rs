//! 事件总线 —— core `CoreEvent` 通过 `ForeignEventBus` callback 回 JS。
//!
//! 关键决策:
//! - 镜像 enum `MobileCoreEvent`(不在共享 crate 上加 uniffi derive)
//! - 事件单向:Rust → JS;JS 没法直接 emit `CoreEvent`
//! - `spawn_event_loop` 在 node 启动时挂载,主循环退出时一并退出

use std::sync::Arc;

use async_trait::async_trait;
use swarmdrop_core::AppResult;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::network::SharedNetRefs;
use swarmdrop_core::transfer::manager::TransferManager;
use swarmdrop_core::transfer::progress::FileTransferStatus;
use swarmdrop_net::{Events, Router};

use crate::history::MobileTransferProjection;
use crate::network::MobileNetworkStatus;
use crate::transfer::MobileTransferOffer;

// ─────────────── 事件 payload 镜像 ───────────────

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileFileProgress {
    pub file_id: u32,
    pub name: String,
    pub size: u64,
    pub transferred: u64,
    pub status: String,
}

impl From<swarmdrop_core::transfer::progress::FileProgressInfo> for MobileFileProgress {
    fn from(f: swarmdrop_core::transfer::progress::FileProgressInfo) -> Self {
        // 穷尽解构（不带 `..`）作为 drift guard：上游 FileProgressInfo 加字段时此处编译失败
        let swarmdrop_core::transfer::progress::FileProgressInfo {
            file_id,
            name,
            size,
            transferred,
            status,
            chunks_done: _,
            total_chunks: _,
        } = f;
        Self {
            file_id,
            name,
            size,
            transferred,
            status: match status {
                FileTransferStatus::Pending => "pending",
                FileTransferStatus::Transferring => "transferring",
                FileTransferStatus::Completed => "completed",
            }
            .to_string(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferProgress {
    pub session_id: String,
    pub direction: String,
    pub total_files: u64,
    pub completed_files: u64,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub speed: f64,
    pub eta: Option<f64>,
    pub files: Vec<MobileFileProgress>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePrepareProgress {
    pub prepared_id: String,
    pub current_file: String,
    pub completed_files: u32,
    pub total_files: u32,
    pub bytes_hashed: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferResumedFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferResumed {
    pub session_id: String,
    pub direction: String,
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<MobileTransferResumedFile>,
    pub total_size: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePairedDevice {
    pub peer_id: String,
    pub device_name: String,
}

// ─────────────── MobileCoreEvent ───────────────

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
    PairedDeviceAdded {
        device: MobilePairedDevice,
    },
    PairedDeviceRemoved {
        peer_id: String,
    },
    /// 本机设备名已变更。`display_name` 是 core 算好的展示名（名字为空则回退
    /// hostname），JS 侧直接用，不要再写一遍 `name || hostname` 的回退。
    DeviceRenamed {
        name: Option<String>,
        display_name: String,
    },
    TransferOfferReceived {
        offer: MobileTransferOffer,
    },
    TransferProgress {
        progress: MobileTransferProgress,
    },
    TransferAccepted {
        session_id: String,
    },
    TransferRejected {
        session_id: String,
        reason: Option<String>,
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
    TransferResumed {
        event: MobileTransferResumed,
    },
    TransferProjectionUpdate {
        projection: MobileTransferProjection,
    },
    TransferDbError {
        session_id: String,
        message: String,
    },
    PrepareProgress {
        event: MobilePrepareProgress,
    },
    Error {
        message: String,
    },
}

#[uniffi::export(with_foreign)]
pub trait ForeignEventBus: Send + Sync {
    fn emit(&self, event: MobileCoreEvent);
}

/// 纯转发：`CoreEvent` → `MobileCoreEvent` → JS。
///
/// **不持有任何存储端口。** 它一度持有 `PairedDeviceStore` 来回写新配对/刷新的设备，
/// 那份职责已经收进 core 的 `PairingManager::commit_paired_device`（与桌面、Web 同一个入口）。
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
        // **两个方向都只转发，host 不再回写。** `PairedDeviceAdded` 到达时 core 的
        // `PairingManager::commit_paired_device` 已经写过盘（配对达成与 identify 刷新
        // 走同一个入口），`PairedDeviceRemoved` 同理由 `unpair` 写过。重复写虽幂等，
        // 却会让持久化失败被第二次成功掩盖 —— 而「写盘成没成」正是上层要如实告诉用户的。
        if let Some(mobile_event) = map_event(event) {
            self.foreign.emit(mobile_event);
        }
        Ok(())
    }
}

fn map_event(event: CoreEvent) -> Option<MobileCoreEvent> {
    let mapped = match event {
        CoreEvent::NetworkStatusChanged { status } => MobileCoreEvent::NetworkStatusChanged {
            status: status.into(),
        },
        CoreEvent::DevicesChanged { .. } => MobileCoreEvent::DevicesChanged,
        CoreEvent::PairingRequestReceived {
            peer_id,
            pending_id,
            request,
        } => {
            // 邀请/Direct 配对无需向 UI 回传凭证——发起方（收到本请求者）已知上下文，
            // 只需展示对端身份并让用户确认。字段保留 None 以稳定 FFI 签名。
            let _ = &request.method;
            MobileCoreEvent::PairingRequestReceived {
                peer_id: peer_id.to_string(),
                pending_id,
                code: None,
            }
        }
        CoreEvent::PairingCompleted { peer_id } => MobileCoreEvent::PairingCompleted { peer_id },
        CoreEvent::PairedDeviceAdded { device } => MobileCoreEvent::PairedDeviceAdded {
            device: MobilePairedDevice {
                peer_id: device.peer_id.to_string(),
                device_name: device
                    .os_info
                    .name
                    .filter(|n| !n.is_empty())
                    .unwrap_or(device.os_info.hostname),
            },
        },
        CoreEvent::PairedDeviceRemoved { peer_id } => MobileCoreEvent::PairedDeviceRemoved {
            peer_id: peer_id.to_string(),
        },
        CoreEvent::DeviceRenamed { name, display_name } => {
            MobileCoreEvent::DeviceRenamed { name, display_name }
        }
        CoreEvent::TransferOfferReceived { offer } => MobileCoreEvent::TransferOfferReceived {
            offer: offer.into(),
        },
        CoreEvent::TransferProgress { event } => MobileCoreEvent::TransferProgress {
            progress: MobileTransferProgress {
                session_id: event.session_id.to_string(),
                direction: format!("{:?}", event.direction).to_lowercase(),
                total_files: event.total_files as u64,
                completed_files: event.completed_files as u64,
                total_bytes: event.total_bytes,
                transferred_bytes: event.transferred_bytes,
                speed: event.speed,
                eta: event.eta,
                files: event.files.into_iter().map(Into::into).collect(),
            },
        },
        CoreEvent::TransferAccepted { event } => MobileCoreEvent::TransferAccepted {
            session_id: event.session_id.to_string(),
        },
        CoreEvent::TransferRejected { event } => MobileCoreEvent::TransferRejected {
            session_id: event.session_id.to_string(),
            reason: event.reason.map(|r| format!("{:?}", r)),
        },
        CoreEvent::TransferCompleted { event } => MobileCoreEvent::TransferCompleted {
            session_id: event.session_id.to_string(),
        },
        CoreEvent::TransferFailed { event } => MobileCoreEvent::TransferFailed {
            session_id: event.session_id.to_string(),
            error: event.error,
        },
        CoreEvent::TransferPaused { event } => MobileCoreEvent::TransferPaused {
            session_id: event.session_id.to_string(),
        },
        CoreEvent::TransferResumed { event } => MobileCoreEvent::TransferResumed {
            event: MobileTransferResumed {
                session_id: event.session_id.to_string(),
                direction: format!("{:?}", event.direction).to_lowercase(),
                peer_id: event.peer_id,
                peer_name: event.peer_name,
                files: event
                    .files
                    .into_iter()
                    .map(|f| MobileTransferResumedFile {
                        file_id: f.file_id,
                        name: f.name,
                        relative_path: f.relative_path,
                        size: f.size,
                        is_directory: f.is_directory,
                    })
                    .collect(),
                total_size: event.total_size,
            },
        },
        CoreEvent::TransferProjection { projection } => MobileCoreEvent::TransferProjectionUpdate {
            projection: projection.into(),
        },
        CoreEvent::TransferDbError { event } => MobileCoreEvent::TransferDbError {
            session_id: event.session_id.to_string(),
            message: event.message,
        },
        CoreEvent::PrepareProgress { event } => MobileCoreEvent::PrepareProgress {
            event: MobilePrepareProgress {
                prepared_id: event.prepared_id.to_string(),
                current_file: event.current_file,
                completed_files: event.completed_files,
                total_files: event.total_files,
                bytes_hashed: event.bytes_hashed,
                total_bytes: event.total_bytes,
            },
        },
        CoreEvent::Error { message } => MobileCoreEvent::Error { message },
        // #[non_exhaustive]：未来新增变体先返回 None，等 mobile 镜像跟上
        _ => return None,
    };
    Some(mapped)
}

/// 事件循环：完整版（包含 Transfer 处理），需要 TransferManager 已就绪。
///
/// 装配（含 `router` 保活）委托 core 的
/// [`spawn_event_loop`](swarmdrop_core::network::event_loop::spawn_event_loop)；本壳只做
/// [`MobileEventBusAdapter`] → `dyn EventBus` 的类型擦除。
pub(crate) fn spawn_event_loop(
    events: Events,
    shared: SharedNetRefs<TransferManager>,
    event_bus: Arc<MobileEventBusAdapter>,
    router: Router,
) {
    swarmdrop_core::network::event_loop::spawn_event_loop(
        events,
        shared,
        event_bus as Arc<dyn EventBus>,
        router,
    );
}
