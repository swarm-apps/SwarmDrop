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
use swarmdrop_core::protocol::PairingMethod;
use swarmdrop_core::transfer::manager::TransferManager;
use swarmdrop_core::transfer::progress::FileTransferStatus;
use swarmdrop_net::{Events, Router};

use crate::history::MobileTransferProjection;
use crate::keychain::MobileKeychainAdapter;
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

pub(crate) struct MobileEventBusAdapter {
    foreign: Arc<dyn ForeignEventBus>,
    /// 用于把新配对 / Identify 刷新后的已配对设备写回 keychain（对齐桌面 host 行为）。
    keychain: Arc<MobileKeychainAdapter>,
}

impl MobileEventBusAdapter {
    pub(crate) fn new(
        foreign: Arc<dyn ForeignEventBus>,
        keychain: Arc<MobileKeychainAdapter>,
    ) -> Self {
        Self { foreign, keychain }
    }
}

#[async_trait]
impl EventBus for MobileEventBusAdapter {
    async fn publish(&self, event: CoreEvent) -> AppResult<()> {
        // PairedDeviceAdded 有两个来源:配对成功时 pairing.rs 主动 publish,以及对端经
        // Identify 广播新设备名时共享 core publish。持久化是 host 职责(桌面在
        // event_bus.rs 里 upsert),移动端在此把设备写回 keychain,使新配对的设备 / 刷新
        // 后的名称在设备离线或应用重启后仍保留。
        if let CoreEvent::PairedDeviceAdded { device } = &event
            && let Err(error) = swarmdrop_core::identity::upsert_paired_device(
                self.keychain.as_ref(),
                device.clone(),
            )
            .await
        {
            tracing::warn!("持久化已配对设备失败: {error}");
        }
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
            let code = match request.method {
                PairingMethod::Code { code } => Some(code),
                PairingMethod::Direct => None,
            };
            MobileCoreEvent::PairingRequestReceived {
                peer_id: peer_id.to_string(),
                pending_id,
                code,
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
