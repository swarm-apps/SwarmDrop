//! Desktop application path adapter.

use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_core::host::{AppPaths, CoreAppPaths};
use tauri::{AppHandle, Manager};

#[derive(Clone)]
pub struct DesktopAppPaths {
    app: AppHandle,
}

impl DesktopAppPaths {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl AppPaths for DesktopAppPaths {
    fn paths(&self) -> CoreResult<CoreAppPaths> {
        let resolver = self.app.path();
        let data_dir = resolver
            .app_data_dir()
            .map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))?;
        let cache_dir = resolver
            .app_cache_dir()
            .map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))?;
        let temp_dir = data_dir.join("temp");
        let log_dir = data_dir.join("logs");

        Ok(CoreAppPaths {
            data_dir,
            cache_dir,
            temp_dir,
            log_dir,
        })
    }
}
