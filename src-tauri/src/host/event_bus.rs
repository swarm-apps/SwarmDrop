//! Tauri event delivery for typed core events.

use std::sync::Arc;

use dashmap::DashMap;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::transfer::progress::PrepareProgressEvent;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter};
use tauri_specta::Event as _;
use uuid::Uuid;

use crate::events::{
    DevicesChanged, NetworkStatusChanged, PairedDeviceAdded, PairingRequestPayload,
    PairingRequestReceived, TransferAccepted, TransferComplete, TransferDbError, TransferFailed,
    TransferOffer, TransferPaused, TransferProgress, TransferRejected, TransferResumed,
};

/// 把 core 的 [`CoreEvent`] 翻译为 Tauri `app.emit(...)`。
///
/// 对于 [`CoreEvent::PrepareProgress`]：根据 `prepared_id` 路由到对应的
/// [`Channel`] 推送给前端（per-call channel，比全局 emit 高效）。
#[derive(Clone)]
pub struct TauriEventBus {
    pub app: AppHandle,
    /// prepared_id → Channel 路由表，commands::prepare_send 中注册/注销
    prepare_channels: Arc<DashMap<Uuid, Channel<PrepareProgressEvent>>>,
}

impl TauriEventBus {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            prepare_channels: Arc::new(DashMap::new()),
        }
    }

    /// 注册 prepare 进度 channel；返回 RAII guard，drop 时自动注销
    pub fn register_prepare_channel(
        &self,
        prepared_id: Uuid,
        channel: Channel<PrepareProgressEvent>,
    ) {
        self.prepare_channels.insert(prepared_id, channel);
    }

    pub fn unregister_prepare_channel(&self, prepared_id: &Uuid) {
        self.prepare_channels.remove(prepared_id);
    }
}

/// `register_prepare_channel` 的 RAII guard：drop 时自动注销
///
/// **Why:** prepare 命令可能因为 await 取消、panic 或异常返回而跳过手动 unregister，
/// guard 保证 channel 不会泄漏在 DashMap 里。
pub struct PrepareChannelGuard {
    prepare_channels: Arc<DashMap<Uuid, Channel<PrepareProgressEvent>>>,
    prepared_id: Uuid,
}

impl PrepareChannelGuard {
    pub fn register(
        bus: &TauriEventBus,
        prepared_id: Uuid,
        channel: Channel<PrepareProgressEvent>,
    ) -> Self {
        bus.prepare_channels.insert(prepared_id, channel);
        Self {
            prepare_channels: bus.prepare_channels.clone(),
            prepared_id,
        }
    }
}

impl Drop for PrepareChannelGuard {
    fn drop(&mut self) {
        self.prepare_channels.remove(&self.prepared_id);
    }
}

#[async_trait::async_trait]
impl EventBus for TauriEventBus {
    async fn publish(&self, event: CoreEvent) -> swarmdrop_core::AppResult<()> {
        let map_err = |e: tauri::Error| swarmdrop_core::AppError::Network(e.to_string());

        match event {
            CoreEvent::NetworkStatusChanged { status } => {
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
            CoreEvent::PairedDeviceAdded { device } => {
                PairedDeviceAdded(device).emit(&self.app).map_err(map_err)?;
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
            CoreEvent::PrepareProgress { event } => {
                // 优先走 per-call channel；没有时退化为全局 emit（fallback 路径）
                if let Some(channel) = self.prepare_channels.get(&event.prepared_id) {
                    let _ = channel.send(event);
                } else {
                    let _ = self.app.emit("prepare-progress", &event);
                }
            }
            CoreEvent::Error { .. } => {}
            // #[non_exhaustive]：未知变体直接忽略
            _ => {}
        }

        Ok(())
    }
}
