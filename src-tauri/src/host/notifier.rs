//! Desktop notification adapter.

use async_trait::async_trait;
use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_core::host::{NotificationRequest, Notifier};
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
    async fn notify(&self, request: NotificationRequest) -> CoreResult<()> {
        self.app
            .notification()
            .builder()
            .title(request.title)
            .body(request.body)
            .show()
            .map_err(|error| CoreError::Network(error.to_string()))
    }

    /// 桌面端：仅当窗口失焦时才弹通知，避免打扰前台操作
    async fn notify_if_unfocused(&self, request: NotificationRequest) -> CoreResult<()> {
        use tauri::Manager;
        let focused = self
            .app
            .webview_windows()
            .values()
            .any(|window| window.is_focused().unwrap_or(false));
        if !focused {
            self.notify(request).await
        } else {
            Ok(())
        }
    }
}
