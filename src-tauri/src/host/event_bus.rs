//! Tauri event delivery for typed core events.

use serde::Serialize;
use swarm_p2p_core::libp2p::PeerId;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::protocol::PairingRequest;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::events;

#[derive(Clone)]
pub struct TauriEventBus {
    pub app: AppHandle,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequestPayload {
    pub peer_id: PeerId,
    pub pending_id: u64,
    #[serde(flatten)]
    pub request: PairingRequest,
}

#[async_trait::async_trait]
impl EventBus for TauriEventBus {
    async fn publish(&self, event: CoreEvent) -> swarmdrop_core::AppResult<()> {
        match event {
            CoreEvent::NetworkStatusChanged { status } => self
                .app
                .emit(events::NETWORK_STATUS_CHANGED, &status)
                .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?,
            CoreEvent::DevicesChanged { devices } => self
                .app
                .emit(events::DEVICES_CHANGED, &devices)
                .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?,
            CoreEvent::PairingRequestReceived {
                peer_id,
                pending_id,
                request,
            } => {
                notify_if_unfocused(
                    &self.app,
                    "配对请求",
                    &format!("{} 请求与您配对", request.os_info.hostname),
                );

                let payload = PairingRequestPayload {
                    peer_id,
                    pending_id,
                    request,
                };
                self.app
                    .emit(events::PAIRING_REQUEST_RECEIVED, &payload)
                    .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?;
            }
            CoreEvent::PairingCompleted { .. } | CoreEvent::TransferProgress { .. } => {}
            CoreEvent::TransferOfferReceived { offer } => {
                self.app
                    .emit(events::TRANSFER_OFFER, &offer)
                    .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?;
                notify_if_unfocused(
                    &self.app,
                    "收到文件传输请求",
                    &format!("{} 想要向您发送文件", offer.device_name),
                );
            }
            CoreEvent::TransferCompleted { event } => self
                .app
                .emit(events::TRANSFER_COMPLETE, &event)
                .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?,
            CoreEvent::TransferFailed { event } => {
                self.app
                    .emit(events::TRANSFER_FAILED, &event)
                    .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?
            }
            CoreEvent::TransferPaused { event } => {
                self.app
                    .emit(events::TRANSFER_PAUSED, &event)
                    .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?
            }
            CoreEvent::TransferDbError { event } => self
                .app
                .emit(events::TRANSFER_DB_ERROR, &event)
                .map_err(|e| swarmdrop_core::AppError::Network(e.to_string()))?,
            CoreEvent::Error { .. } => {}
        }

        Ok(())
    }
}

pub fn notify_if_unfocused(app: &AppHandle, title: &str, body: &str) {
    let focused = app
        .webview_windows()
        .values()
        .any(|window| window.is_focused().unwrap_or(false));
    if !focused {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}
