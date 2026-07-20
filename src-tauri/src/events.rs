//! Tauri 类型化事件
//!
//! 用 newtype + `#[serde(transparent)]` 包装 core payload：wire 形状不变，
//! 同时让 tauri-specta 把 struct ident 自动转 kebab-case 作为事件名
//! （`NetworkStatusChanged` → `"network-status-changed"`）。

use serde::Serialize;
use swarmdrop_core::device::{Device, PairedDeviceInfo};
use swarmdrop_core::network::NetworkStatus;
use swarmdrop_core::transfer::incoming::TransferOfferEvent;
use swarmdrop_core::transfer::progress::{
    TransferAcceptedEvent, TransferCompleteEvent, TransferDbErrorEvent, TransferFailedEvent,
    TransferPausedEvent, TransferProgressEvent, TransferRejectedEvent, TransferResumedEvent,
};
use swarmdrop_core::transfer::store::TransferProjection;

// === 网络状态 ===

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct NetworkStatusChanged(pub NetworkStatus);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct DevicesChanged(pub Vec<Device>);

// === 配对 ===

/// 配对请求 payload：原 core 事件含 PeerId（非 specta-friendly），在此 host 层
/// 投影成 `String`，并把 `request` 字段 flatten 摊开（保持原 wire 形状）。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequestPayload {
    pub peer_id: String,
    pub pending_id: u64,
    #[serde(flatten)]
    pub request: swarmdrop_core::protocol::PairingRequest,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct PairingRequestReceived(pub PairingRequestPayload);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct PairedDeviceAdded(pub PairedDeviceInfo);

// === 传输 ===

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferOffer(pub TransferOfferEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferProgress(pub TransferProgressEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferAccepted(pub TransferAcceptedEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferRejected(pub TransferRejectedEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferComplete(pub TransferCompleteEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferFailed(pub TransferFailedEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferPaused(pub TransferPausedEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferResumed(pub TransferResumedEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferDbError(pub TransferDbErrorEvent);

/// 传输投影更新（redesign：前端唯一状态源）。事件名 `"transfer-projection-update"`。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferProjectionUpdate(pub TransferProjection);

// === 接收暂停 ===

/// 全局「暂停接收」状态变更（托盘 / 命令切换后广播，供 UI 与托盘同步）。
/// 事件名 `"receiving-paused-changed"`，payload 为 `true`=已暂停。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct ReceivingPausedChanged(pub bool);

// === 外部文件打开（Open With → share-target 反向发送）===

/// 外部「用 SwarmDrop 打开」文件/文件夹后归一化的本地绝对路径列表。
/// 事件名 `"external-file-open"`，前端根处理器据此扫描并跳转选设备屏。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ExternalFileOpen {
    pub paths: Vec<String>,
}

// === 托盘信号（Rust 托盘 → 前端执行依赖前端状态的动作）===

/// 托盘「打开接收文件夹」：路径由前端 `savePath` 拥有，故由前端打开。
/// 事件名 `"tray-open-receive-folder"`。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct TrayOpenReceiveFolder;

/// 托盘「设置」：由前端路由跳转到设置页。事件名 `"tray-open-settings"`。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct TrayOpenSettings;
