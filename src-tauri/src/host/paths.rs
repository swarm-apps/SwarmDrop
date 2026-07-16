//! app 数据目录解析。
//!
//! 唯一目的：给 demo / 录制 fixture 提供一个 **非破坏性** 的数据目录切换点——
//! 设置 `SWARMDROP_DATA_DIR` 后，identity / device_config / SQLite 全部落到该
//! 目录，真实用户 profile 零接触。**仅 debug build 生效**，release 永远走平台
//! 默认目录，避免把 fixture 覆盖带进生产。
//!
//! 三个调用方（[`file_keychain`](super::file_keychain) /
//! [`device_config`](super::device_config) / [`crate::database`]）统一经此取目录，
//! `SWARMDROP_DATA_DIR` 的判断只在这里发生。

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// identity / device_config 用的数据目录（默认平台 `app_data_dir`）。
pub fn app_data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    if let Some(dir) = fixture_dir() {
        return Ok(dir);
    }
    app.path().app_data_dir()
}

/// database 用的本地数据目录（默认平台 `app_local_data_dir`）。
/// fixture 覆盖时与 [`app_data_dir`] 归到同一目录，让一份 fixture 自洽。
pub fn app_local_data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    if let Some(dir) = fixture_dir() {
        return Ok(dir);
    }
    app.path().app_local_data_dir()
}

/// 读取 `SWARMDROP_DATA_DIR` fixture 覆盖；仅 debug build 生效，且目录不可创建时
/// 回落为 `None`（走平台默认），绝不因 fixture 配置错误而中断启动。
fn fixture_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let raw = std::env::var("SWARMDROP_DATA_DIR").ok()?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let path = PathBuf::from(trimmed);
        if let Err(err) = std::fs::create_dir_all(&path) {
            tracing::warn!(
                "[paths] SWARMDROP_DATA_DIR {} 无法创建，回落默认目录: {err}",
                path.display()
            );
            return None;
        }
        Some(path)
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}
