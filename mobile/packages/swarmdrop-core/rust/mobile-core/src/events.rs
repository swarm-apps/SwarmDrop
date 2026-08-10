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
use swarmdrop_core::transfer::progress::{FilePublishPhase, FileTransferStatus};
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

/// 发布阶段。**fieldless `uniffi::Enum`，与同域的 [`MobileTransferDirection`] 等同体例**
/// ——生成的 TS 是 `export enum MobileFilePublishPhase { Started, Finished }`，所以 JS 侧
/// 加档时 switch 会缺项报错。
///
/// 早先这里是 `String`，理由写作「uniffi enum 会生成 `{ tag: … }` 对象」——**那是带字段
/// 枚举的形态**，无字段的不是。裸 `String` 让 `FilePublishEvent` 的文档断言（「三端 codegen
/// 都稳，加档会在查表处编译期报缺项」）恰好在唯一有慢发布路径的那一端不成立。
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileFilePublishPhase {
    Started,
    Finished,
}

impl From<FilePublishPhase> for MobileFilePublishPhase {
    fn from(phase: FilePublishPhase) -> Self {
        match phase {
            FilePublishPhase::Started => Self::Started,
            FilePublishPhase::Finished => Self::Finished,
        }
    }
}

/// 单个文件的发布阶段（暂存 → 用户可见位置）。
///
/// **Android 上这一段是全量字节拷贝，几十秒起步**，而此时字节已收完、进度条已满——
/// 没有这条事件，用户看到的就是「满了之后凭空多等一段」。
///
/// 拷贝中的字节数**不在这里**——那个循环在 JS 侧的 `ForeignFileAccess` 里，由它直接上报。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileFilePublish {
    pub session_id: String,
    pub file_id: u32,
    pub name: String,
    /// JS 侧靠它把自己的拷贝字节数认领到正确的条目上——它拿到的元数据里没有会话与文件 id。
    pub relative_path: String,
    pub total_bytes: u64,
    pub phase: MobileFilePublishPhase,
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
    FilePublish {
        event: MobileFilePublish,
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
        CoreEvent::FilePublish { event } => MobileCoreEvent::FilePublish {
            event: MobileFilePublish {
                session_id: event.session_id.to_string(),
                file_id: event.file_id,
                name: event.name,
                relative_path: event.relative_path,
                total_bytes: event.total_bytes,
                phase: event.phase.into(),
            },
        },
        CoreEvent::Error { message } => MobileCoreEvent::Error { message },
        // `CoreEvent` 是 `#[non_exhaustive]`，所以漏接一个变体**不会**编译失败——这条兜底
        // 是移动端唯一会被静默吞掉的路径。**必须留日志**：否则症状是「功能在桌面好用、
        // 在手机上什么都不发生」，而代码看起来完全正常。
        other => {
            tracing::warn!("mobile 事件镜像未覆盖的 CoreEvent，已丢弃: {other:?}");
            return None;
        }
    };
    Some(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 发布事件必须真的过得了镜像这一关。
    ///
    /// 它是本文件里唯一**不会编译期报错**的接线点：`map_event` 的兜底分支会把漏接的变体
    /// 静默吞掉，而 UI 侧只会表现为「Android 保存时那段提示永远不出现」。
    #[test]
    fn file_publish_should_survive_the_mobile_mirror() {
        let session_id = uuid::Uuid::new_v4();
        let mapped = map_event(CoreEvent::FilePublish {
            event: swarmdrop_core::transfer::progress::FilePublishEvent {
                session_id,
                file_id: 7,
                name: "a.bin".to_string(),
                relative_path: "sub/a.bin".to_string(),
                total_bytes: 1024,
                phase: FilePublishPhase::Started,
            },
        })
        .expect("发布事件必须有镜像");

        let MobileCoreEvent::FilePublish { event } = mapped else {
            panic!("映射到了错误的变体");
        };
        assert_eq!(event.session_id, session_id.to_string());
        assert_eq!(event.file_id, 7);
        assert_eq!(event.relative_path, "sub/a.bin");
        assert_eq!(event.phase, MobileFilePublishPhase::Started);
    }
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
