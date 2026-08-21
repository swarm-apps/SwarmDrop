//! Tauri event delivery for typed core events.

use swarmdrop_core::host::{CoreEvent, EventBus};
use tauri::AppHandle;
use tauri_specta::Event as _;

use crate::events::{
    DeviceRenamed, DevicesChanged, FilePublish, InboxItemAdded, InboxItemArchived,
    InboxItemRemoved, NetworkStatusChanged, PairedDeviceAdded, PairedDeviceRemoved,
    PairingRequestPayload, PairingRequestReceived, PrepareProgress, TextDeliveryAttentionReceived,
    TransferAccepted, TransferComplete, TransferDbError, TransferFailed, TransferOffer,
    TransferPaused, TransferProgress, TransferProjectionUpdate, TransferRejected, TransferResumed,
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
            CoreEvent::TextDeliveryAttention { attention } => {
                TextDeliveryAttentionReceived(attention)
                    .emit(&self.app)
                    .map_err(map_err)?;
            }
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
            CoreEvent::FilePublish { event } => {
                FilePublish(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::InboxItemAdded { event } => {
                InboxItemAdded(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::InboxItemArchived { event } => {
                InboxItemArchived(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::InboxItemRemoved { event } => {
                InboxItemRemoved(event).emit(&self.app).map_err(map_err)?;
            }
            CoreEvent::Error { .. } => {}
            // `CoreEvent` 是 `#[non_exhaustive]`，所以漏接一个变体**不会**编译失败。
            // **必须留日志**：否则症状是「事件发了、前端没反应」，而两边代码都看着正常。
            other => {
                tracing::warn!("桌面事件转发未覆盖的 CoreEvent，已丢弃: {other:?}");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// **收件箱事件必须真的接在这个镜像上。**
    ///
    /// 这是本文件里唯一无法靠类型保证的接线点，且两条路都堵死：`publish` 要 `AppHandle`
    /// 才能跑，而 `CoreEvent` 是 `#[non_exhaustive]`——跨 crate 的 match **必须**带兜底分支，
    /// 编译器因此永远不会因为漏接而报错。漏接的症状是「文件收到了、收件箱页不动」，
    /// 而两边代码都看着正常。
    ///
    /// 这条尤其要钉住：接上它正是为了替换掉旧的推导路径，而桌面此前**根本没有**推导
    /// （`inbox-store.ts` 零事件监听）。一旦被吞，收件箱就回到「永远要手动刷新」。
    ///
    /// 只能做到源码级断言——但它锁住的是「有人把这几行删了或改了名」这件事，
    /// 那正是真实会发生的破坏方式。
    #[test]
    fn inbox_events_are_wired_into_the_desktop_mirror() {
        let src = include_str!("event_bus.rs");
        for variant in ["InboxItemAdded", "InboxItemArchived", "InboxItemRemoved"] {
            let arm = format!("CoreEvent::{variant}");
            assert!(
                src.contains(&arm),
                "{variant} 没有接在桌面事件镜像上，它会落进兜底分支被静默丢弃——见本测试的文档注释"
            );
        }
    }
}
