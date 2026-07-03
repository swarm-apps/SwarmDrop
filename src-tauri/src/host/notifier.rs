//! Desktop notification adapter.

use async_trait::async_trait;
use rust_i18n::t;
use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_core::host::{Notification, Notifier};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

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
        // 语义通知在此按当前 locale 翻译成标题/正文（core 不含语言散文）。
        let (title, body) = localize(&notification);
        self.app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|error| CoreError::Network(error.to_string()))
    }

    /// 桌面端：仅当窗口失焦时才弹通知，避免打扰前台操作
    async fn notify_if_unfocused(&self, notification: Notification) -> CoreResult<()> {
        use tauri::Manager;
        let focused = self
            .app
            .webview_windows()
            .values()
            .any(|window| window.is_focused().unwrap_or(false));
        if !focused {
            self.notify(notification).await
        } else {
            Ok(())
        }
    }
}

/// 语义通知 → 当前 locale 的（标题, 正文）。词条见 `src-tauri/locales/*.toml` 的 `notif.*`。
fn localize(notification: &Notification) -> (String, String) {
    match notification {
        Notification::PairingRequest { hostname } => (
            t!("notif.pairing.title").to_string(),
            t!("notif.pairing.body", hostname = hostname).to_string(),
        ),
        Notification::IncomingTransfer { device_name } => (
            t!("notif.incoming_transfer.title").to_string(),
            t!("notif.incoming_transfer.body", name = device_name).to_string(),
        ),
    }
}
