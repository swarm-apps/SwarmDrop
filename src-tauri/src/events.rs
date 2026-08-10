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
    FilePublishEvent, PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent,
    TransferDbErrorEvent, TransferFailedEvent, TransferPausedEvent, TransferProgressEvent,
    TransferRejectedEvent, TransferResumedEvent,
};
use swarmdrop_core::transfer::store::TransferProjection;

// === 网络状态 ===

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct NetworkStatusChanged(pub NetworkStatus);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct DevicesChanged(pub Vec<Device>);

// === 本机设备 ===

/// 本机设备名已更新（落盘 + identify 广播都已完成）。事件名 `"device-renamed"`。
///
/// 前端更新设备名镜像的**唯一**入口：改名可能来自另一个窗口或 MCP 工具，让发起改名的
/// 那个界面自己刷新覆盖不到这些来源。`displayName` 已含「空则回退 hostname」的语义
/// （core 的 `OsInfo::display_name()`），前端不必再写一遍那个回退。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRenamed {
    pub name: Option<String>,
    pub display_name: String,
}

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

/// 已解除配对的设备 PeerId（base58）。事件名 `"paired-device-removed"`。
///
/// 它是前端移除该设备的**唯一**入口：命令自己不再顺手改本地状态，否则同一条记录
/// 会被两条路径各删一次，谁先谁后取决于时序。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct PairedDeviceRemoved(pub String);

// === 传输 ===

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferOffer(pub TransferOfferEvent);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TransferProgress(pub TransferProgressEvent);

/// 发送前置准备（一遍流式读产出 checksum + 验签树）的进度。事件名 `"prepare-progress"`。
///
/// **按 `preparedId` 广播，不走 per-call channel。** 此前它是全仓唯一的
/// `tauri::ipc::Channel`（21 个 typed event : 1 个 channel），而那不是权衡的产物——
/// 它出生于 2026-02（`ff47e1dd`），比 tauri-specta typed events 引入早三个月，当时那个
/// 选项根本不存在；2026-05 的 typed events 迁移把它漏下了，commit body 里没有任何理由。
///
/// 换成广播修掉三条静默路径，它们是同一处修复：
/// - **MCP `send_files`**（`mcp/tools.rs`）自己 mint `prepared_id`，没有 invoke 生命周期
///   可挂 channel，进度事件此前 100% 被丢弃——对「Agent 发文件」这个定位尤其难受；
/// - 前端离开发送页即卸载 channel 消费者，回来时进度无从恢复；
/// - 并发 prepare 需要按 id 区分，channel 表能做但没人读。
///
/// 注意这条事件**没有 `sessionId`**：会话记录要等 prepare 跑完、发出 Offer 时才创建，
/// 所以它不能挂进任何按会话索引的状态。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct PrepareProgress(pub PrepareProgressEvent);

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

/// 单个文件正在从暂存位置发布到用户可见位置。事件名 `"file-publish"`。
///
/// 桌面的发布是同目录 `rename`（O(1)），这条事件通常一闪而过——它在这里是为了让三端对
/// 「字节收完 ≠ 文件落地」有同一套表达，而不是因为桌面会卡在这一步。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct FilePublish(pub FilePublishEvent);

// === 接收暂停 ===

/// 全局「暂停接收」状态变更（托盘 / 命令切换后广播，供 UI 与托盘同步）。
/// 事件名 `"receiving-paused-changed"`，payload 为 `true`=已暂停。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct ReceivingPausedChanged(pub bool);

// === 外部入口（Open With → share-target 反向发送；深链 → 配对）===

/// 外部「用 SwarmDrop 打开」文件/文件夹后归一化的本地绝对路径列表。
/// 事件名 `"external-file-open"`，前端根处理器据此扫描并跳转选设备屏。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ExternalFileOpen {
    pub paths: Vec<String>,
}

/// 深链（`swarmdrop://…`）送达的配对邀请链接原文。
///
/// 事件名 `"external-pair-invite"`。**未解码未验签** —— 宿主层只递文本，前端照常走
/// 「解码验签 → 确认卡 → 用户确认」的安全闸，与扫码/粘贴同一条路
/// （openspec: pair-deep-link）。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ExternalPairInvite {
    pub invite: String,
}

// === 托盘信号（Rust 托盘 → 前端执行依赖前端状态的动作）===

/// 托盘「打开接收文件夹」：路径由前端 `savePath` 拥有，故由前端打开。
/// 事件名 `"tray-open-receive-folder"`。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct TrayOpenReceiveFolder;

/// 托盘「设置」：由前端路由跳转到设置页。事件名 `"tray-open-settings"`。
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct TrayOpenSettings;
