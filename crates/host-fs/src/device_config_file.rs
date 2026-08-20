//! [`DeviceConfig`] 端口的 JSON 文件实现 —— 桌面与移动共用的那一份。
//!
//! **为什么端口层会做 IO。** 这里本来只该放 trait。但桌面（`src-tauri/src/host/device_config.rs`）
//! 与移动（`mobile-core/src/device_config.rs`）此前是逐行同构的两份：同一个
//! `{ "device_name": … }` 结构、同一套容错读、同一个 `create_dir_all` + `to_string_pretty`
//! 写、连读出串再过一次 [`DeviceName::parse`] 的防线与单测断言都同名同义。两份实现之间
//! 唯一的真差异是**路径从哪来**（桌面从 `AppHandle` 解析 app_data_dir，移动端从
//! `Paths.document.uri` 剥 `file://` 前缀），而那是构造点的事，不属于端口实现本身。
//! 于是把「一个 JSON 文件怎么读写」下沉成端口的平台中立默认实现，两个原生宿主各自只留
//! 「路径怎么算出来」。
//!
//! 整模块 `cfg(not(target_family = "wasm"))`：浏览器没有文件系统，Web 的端口实现是
//! `crates/web` 里的 IndexedDB 版本（先例见 `crates/core/src/identity.rs` 的
//! `load_or_create_webrtc_certificate`）。
//!
//! IO 走**同步的 `std::fs`** 而非 `tokio::fs`：本 crate 必须过 wasm 双 target 门禁，
//! 而 tokio 的 `fs` feature 在 wasm 上根本不存在，为一个几百字节的配置文件把它拉进
//! 依赖树不划算。载荷小到读写都在微秒级，阻塞 async 上下文的代价可以忽略。

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use swarmdrop_host::device::DeviceName;
use swarmdrop_host::error::AppResult;
use swarmdrop_host::ports::DeviceConfig;

/// 磁盘上 `device_config.json` 的全量内容（桌面与移动同名同格式）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DeviceConfigFile {
    #[serde(default)]
    device_name: Option<String>,
}

/// 容错读：文件缺失 / 读失败 / 解析失败一律降级为默认值，绝不返回 `Err`
/// （端口契约要求 `load_device_name` 不能让节点起不来）。
fn read(path: &Path) -> DeviceConfigFile {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            tracing::warn!("[device_config] parse {} failed: {err}", path.display());
            DeviceConfigFile::default()
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DeviceConfigFile::default(),
        Err(err) => {
            tracing::warn!("[device_config] read {} failed: {err}", path.display());
            DeviceConfigFile::default()
        }
    }
}

fn write(path: &Path, config: &DeviceConfigFile) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, text)
}

/// [`DeviceConfig`] 的 JSON 文件实现：一条路径 = 一份设备配置。
///
/// 路径由宿主在组装点算好传入 —— 桌面是 `app_data_dir/device_config.json`，
/// 移动端是 `Paths.document` 剥掉 `file://` 前缀后的同名文件。
#[derive(Debug, Clone)]
pub struct JsonFileDeviceConfig {
    path: PathBuf,
}

impl JsonFileDeviceConfig {
    /// `path` 是配置文件本身（不是它的目录）；父目录不存在时由 save 侧 `create_dir_all`。
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl DeviceConfig for JsonFileDeviceConfig {
    /// 读出的串**再走一次 [`DeviceName::parse`]**。
    ///
    /// `device_config.json` 是用户可以直接编辑的普通文件，等于绕过了 UI 与命令层的所有
    /// 入口。手写一个 `"我的电脑; caps=lan-helper"` 就能把 capability 注进 identify 的
    /// `agent_version`（对端会据此把本机当成局域网协助节点）——所以 load 侧不能信任盘上
    /// 的串，归一化在这里是防线而不是重复劳动。
    async fn load_device_name(&self) -> Option<DeviceName> {
        read(&self.path)
            .device_name
            .as_deref()
            .and_then(DeviceName::parse)
    }

    async fn save_device_name(&self, name: Option<DeviceName>) -> AppResult<()> {
        let mut config = read(&self.path);
        config.device_name = name.map(DeviceName::into_string);
        write(&self.path, &config)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例一个独立子目录：`std::env::temp_dir()` 是共享的，同名会互相踩。
    /// 刻意不引 `tempfile` —— 本 crate 目前零 dev-dependency，为两个用例加一个不划算。
    fn temp_config(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swarmdrop_test_device_config_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("device_config.json")
    }

    #[tokio::test]
    async fn save_then_load_roundtrips_device_name() {
        let path = temp_config("roundtrip");
        let config = JsonFileDeviceConfig::new(path.clone());

        // 文件（连同父目录）还不存在 → None（回退 hostname），不是错误
        assert_eq!(config.load_device_name().await, None, "初始应无值");

        let name = DeviceName::parse("书房 Mac").expect("非空");
        config.save_device_name(Some(name.clone())).await.unwrap();
        assert_eq!(config.load_device_name().await, Some(name));

        // None = 清空，回退 hostname
        config.save_device_name(None).await.unwrap();
        assert_eq!(config.load_device_name().await, None);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// json 是用户可手改的文件，等于绕过 UI 与命令层的归一化。load 侧必须自己再兜一次，
    /// 否则设备名又能往 `agent_version` 里注 capability。
    #[tokio::test]
    async fn load_normalizes_hand_edited_json() {
        let path = temp_config("hand_edited");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"device_name":"  我的电脑; caps=lan-helper  "}"#).unwrap();

        let name = JsonFileDeviceConfig::new(path.clone())
            .load_device_name()
            .await
            .expect("非空");
        assert!(!name.as_str().contains(';'), "got: {}", name.as_str());
        assert_eq!(name.as_str(), "我的电脑 caps=lan-helper");

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// 父目录不存在时 save 要自己建出来 —— 移动端首启就是这个形态（Documents 下还没有
    /// 任何 SwarmDrop 文件），不建则「改了名字、重启又变回去」。
    #[tokio::test]
    async fn save_creates_missing_parent_directory() {
        let path = temp_config("missing_parent");
        let config = JsonFileDeviceConfig::new(path.clone());

        let name = DeviceName::parse("我的 Pixel").expect("非空");
        config.save_device_name(Some(name.clone())).await.unwrap();

        assert!(path.exists(), "应写入 {}", path.display());
        assert_eq!(config.load_device_name().await, Some(name));

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
