//! 桌面系统通知适配器。

use async_trait::async_trait;
use rust_i18n::t;
use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_core::host::{
    Notification, Notifier, SystemNotification, SystemNotificationPublisher,
    publish_if_window_unfocused,
};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

#[derive(Clone)]
struct TauriNotificationPublisher {
    app: AppHandle,
}

#[async_trait]
impl SystemNotificationPublisher for TauriNotificationPublisher {
    async fn show(&self, notification: SystemNotification) -> CoreResult<()> {
        self.app
            .notification()
            .builder()
            .title(notification.title)
            .body(notification.body)
            .show()
            .map_err(|error| CoreError::Network(error.to_string()))
    }
}

#[derive(Clone)]
pub struct DesktopNotifier {
    app: AppHandle,
}

impl DesktopNotifier {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl Notifier for DesktopNotifier {
    async fn notify(&self, notification: Notification) -> CoreResult<()> {
        let publisher = TauriNotificationPublisher {
            app: self.app.clone(),
        };
        publisher.show(localize(&notification)).await
    }

    /// 仅当所有窗口均未聚焦时显示系统通知，避免打扰正在操作应用的用户。
    async fn notify_if_unfocused(&self, notification: Notification) -> CoreResult<()> {
        use tauri::Manager;

        let has_focused_window = self
            .app
            .webview_windows()
            .values()
            .any(|window| window.is_focused().unwrap_or(false));
        let publisher = TauriNotificationPublisher {
            app: self.app.clone(),
        };
        publish_if_window_unfocused(has_focused_window, localize(&notification), &publisher).await
    }
}

/// 领域通知在展示边缘本地化；`IncomingText` 没有正文输入，因此不可能进入系统通知。
fn localize(notification: &Notification) -> SystemNotification {
    match notification {
        Notification::PairingRequest { hostname } => SystemNotification {
            title: t!("notif.pairing.title").to_string(),
            body: t!("notif.pairing.body", hostname = hostname).to_string(),
        },
        Notification::IncomingTransfer { device_name } => SystemNotification {
            title: t!("notif.incoming_transfer.title").to_string(),
            body: t!("notif.incoming_transfer.body", name = device_name).to_string(),
        },
        Notification::IncomingText { device_name } => SystemNotification {
            title: t!("notif.incoming_text.title").to_string(),
            body: t!("notif.incoming_text.body", name = device_name).to_string(),
        },
    }
}
