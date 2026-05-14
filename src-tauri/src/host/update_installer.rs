//! Desktop update installer adapter.

use async_trait::async_trait;
use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_core::host::{UpdateInstallRequest, UpdateInstaller};
use tauri::AppHandle;

#[cfg(not(target_os = "android"))]
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone)]
pub struct DesktopUpdateInstaller {
    app: AppHandle,
}

impl DesktopUpdateInstaller {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl UpdateInstaller for DesktopUpdateInstaller {
    async fn install_update(&self, request: UpdateInstallRequest) -> CoreResult<()> {
        #[cfg(target_os = "android")]
        {
            let _ = request;
            return Err(CoreError::Network(
                "desktop update installer is not available on Android".into(),
            ));
        }

        #[cfg(not(target_os = "android"))]
        {
            let mut builder = self.app.updater_builder();
            if !request.url.trim().is_empty() {
                let endpoint = url::Url::parse(&request.url)
                    .map_err(|error| CoreError::Network(error.to_string()))?;
                builder = builder
                    .endpoints(vec![endpoint])
                    .map_err(|error| CoreError::Network(error.to_string()))?;
            }

            let Some(update) = builder
                .build()
                .map_err(|error| CoreError::Network(error.to_string()))?
                .check()
                .await
                .map_err(|error| CoreError::Network(error.to_string()))?
            else {
                return Ok(());
            };

            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|error| CoreError::Network(error.to_string()))
        }
    }
}
