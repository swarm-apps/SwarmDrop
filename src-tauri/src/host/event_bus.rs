//! Tauri event delivery for typed core events.

use swarmdrop_core::host::{CoreEvent, EventBus};
use tauri::AppHandle;
use tauri_specta::Event as _;

use crate::events::{
    DeviceRenamed, DevicesChanged, NetworkStatusChanged, PairedDeviceAdded, PairedDeviceRemoved,
    PairingRequestPayload, PairingRequestReceived, PrepareProgress, TransferAccepted,
    TransferComplete, TransferDbError, TransferFailed, TransferOffer, TransferPaused,
    TransferProgress, TransferProjectionUpdate, TransferRejected, TransferResumed,
};

/// 把 core 的 [`CoreEvent`] 翻译成 tauri-specta 的 typed event 广播。
///
/// **全部走广播，一个 per-call channel 都没有。** `PrepareProgress` 曾是唯一的例外
/// （`DashMap<Uuid, Channel>` + RAII guard 路由到发起那次 invoke 的前端），理由见
/// [`crate::events::PrepareProgress`] 的文档——简言之那不是权衡的结果，而是它比 typed
/// events 早生三个月、后来的迁移又把它漏下了。副作用是 MCP 发起的 prepare 进度 100%
/// 被静默丢弃（它没有 invoke 可挂 channel）。
#[derive(Clone)]
pub struct TauriEventBus {
    pub app: AppHandle,
}

impl TauriEventBus {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl EventBus for TauriEventBus {
    async fn publish(&self, event: CoreEvent) -> swarmdrop_core::AppResult<()> {
        let map_err = |e: tauri::Error| swarmdrop_core::AppError::Network(e.to_string());

        match event {
            CoreEvent::NetworkStatusChanged { status } => {
                // 托盘的「在线」此前只由生命周期命令写死传入，说不出「它连得上东西吗」。
                // 这里是唯一每次状态变化都会经过的点，健康度顺路搭在同一条推送上，
                // 不另开轮询。
                crate::tray::refresh_tray_health(
                    &self.app,
                    crate::node_health::summarize(&status, chrono::Utc::now()).is_isolated(),
                );
                NetworkStatusChanged(status)
                    .emit(&self.app)
                    .map_err(map_err)?;
            }
            CoreEvent::DevicesChanged { devices } => {
                DevicesChanged(devices).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::PairingRequestReceived {
                peer_id,
                pending_id,
                request,
            } => {
                let payload = PairingRequestPayload {
                    peer_id: peer_id.to_string(),
                    pending_id,
                    request,
                };
                PairingRequestReceived(payload)
                    .emit(&self.app)
                    .map_err(map_err)?;
            }
            CoreEvent::PairingCompleted { .. } => {}
            // 新增/刷新方向只转发通知：持久化已由 core 的
            // `PairingManager::commit_paired_device` 写过，且它发来的 `device` 就是**合并后**
            // 的版本（保留了用户设过的信任级别与收件策略）。这里再 upsert 一次是第二条写路径，
            // 会让「写盘失败」被第二次成功掩盖 —— 与下面移除方向同一条理由。
            CoreEvent::PairedDeviceAdded { device } => {
                PairedDeviceAdded(device).emit(&self.app).map_err(map_err)?;
            }
            // 移除方向只转发通知：持久化已由 `PairingManager::unpair` 写过，
            // 这里再删一次会让「写盘失败」被第二次成功掩盖。
            CoreEvent::PairedDeviceRemoved { peer_id } => {
                PairedDeviceRemoved(peer_id.to_string())
                    .emit(&self.app)
                    .map_err(map_err)?;
            }
            CoreEvent::DeviceRenamed { name, display_name } => {
                DeviceRenamed { name, display_name }
                    .emit(&self.app)
                    .map_err(map_err)?;
            }
            CoreEvent::TransferOfferReceived { offer } => {
                TransferOffer(offer).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferProgress { event } => {
                TransferProgress(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferAccepted { event } => {
                TransferAccepted(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferRejected { event } => {
                TransferRejected(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferCompleted { event } => {
                TransferComplete(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferFailed { event } => {
                TransferFailed(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferPaused { event } => {
                TransferPaused(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferResumed { event } => {
                TransferResumed(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferDbError { event } => {
                TransferDbError(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::TransferProjection { projection } => {
                TransferProjectionUpdate(projection)
                    .emit(&self.app)
                    .map_err(map_err)?;
            }
            CoreEvent::PrepareProgress { event } => {
                PrepareProgress(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::Error { .. } => {}
            // #[non_exhaustive]：未知变体直接忽略
            _ => {}
        }

        Ok(())
    }
}
