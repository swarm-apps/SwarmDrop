//! 设备级偏好持久化（当前只有 device_name）。
//!
//! 与敏感数据（identity / paired_devices 走 keychain）区分开：device_name 是
//! 用户在 onboarding / 设置里起的名字，无加密需求，存在 `app_data_dir` 下的
//! 普通 JSON 文件即可，方便用户/支持自查。
//!
//! 设计上故意不暴露 trait —— 调用方只通过 [`load_device_name`] /
//! [`save_device_name`] 两个函数交互，所有 IO 错误降级为日志（启动期失败时
//! 节点仍能用 hostname 兜底）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::fs;
use tracing::warn;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DeviceConfig {
    #[serde(default)]
    device_name: Option<String>,
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    match app.path().app_data_dir() {
        Ok(dir) => Some(dir.join("device_config.json")),
        Err(err) => {
            warn!("[device_config] resolve app_data_dir failed: {err}");
            None
        }
    }
}

async fn read(app: &AppHandle) -> DeviceConfig {
    let Some(path) = config_path(app) else {
        return DeviceConfig::default();
    };
    match fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            warn!("[device_config] parse {} failed: {err}", path.display());
            DeviceConfig::default()
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DeviceConfig::default(),
        Err(err) => {
            warn!("[device_config] read {} failed: {err}", path.display());
            DeviceConfig::default()
        }
    }
}

async fn write(app: &AppHandle, cfg: &DeviceConfig) -> std::io::Result<()> {
    let Some(path) = config_path(app) else {
        return Err(std::io::Error::other("app_data_dir unavailable"));
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let text = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&path, text).await
}

/// 节点启动时读取持久化的设备名；缺失/损坏时返回 `None`，由 core 回落到
/// hostname 字段。
pub async fn load_device_name(app: &AppHandle) -> Option<String> {
    read(app).await.device_name
}

/// 写入设备名。`Some` 设置；`None` 清空回退到 hostname。
pub async fn save_device_name(app: &AppHandle, name: Option<String>) -> std::io::Result<()> {
    let mut cfg = read(app).await;
    cfg.device_name = name;
    write(app, &cfg).await
}
