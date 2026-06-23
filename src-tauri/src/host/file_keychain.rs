//! Dev-only 身份文件后端。
//!
//! WARNING: 以**明文**存储 Ed25519 私钥到 `app_data_dir/dev-identity.json`，
//! 仅用于 debug build 绕开 `pnpm tauri dev` 的 ad-hoc 签名二进制无法访问 macOS
//! login keychain 的限制（`errSecInteractionNotAllowed` /
//! "User interaction is not allowed"）。release build 永不编译本模块
//! （见 [`crate::host`] 的 `#[cfg(debug_assertions)]` 门控），生产仍走系统 keychain。

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use swarmdrop_core::device::PairedDeviceInfo;
use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_core::host::{DeviceIdentityBytes, IdentityMigrationState, KeychainProvider};
use tauri::{AppHandle, Manager};
use tokio::fs;
use tracing::warn;

const FILE_NAME: &str = "dev-identity.json";

/// dev 身份文件的全量内容（单文件承载 keychain 后端的全部状态）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevIdentityFile {
    /// protobuf-encoded Ed25519 keypair；空 Vec 视为"无身份"，触发 core 生成新身份。
    #[serde(default)]
    keypair: Vec<u8>,
    #[serde(default)]
    migration_completed: bool,
    #[serde(default)]
    paired_devices: Vec<PairedDeviceInfo>,
}

/// 文件后端的 [`KeychainProvider`] 实现（仅 debug build）。
///
/// 与 [`DesktopKeychainProvider`](crate::host::keychain::DesktopKeychainProvider)
/// 各自单一职责：前者文件、后者系统 keychain，由 [`crate::host::keychain_provider`]
/// 工厂在编译期二选一。
#[derive(Debug, Clone)]
pub struct FileKeychainProvider {
    path: PathBuf,
}

impl FileKeychainProvider {
    pub fn new(app: &AppHandle) -> CoreResult<Self> {
        let dir = app.path().app_data_dir().map_err(|e| {
            CoreError::Identity(format!("dev identity: app_data_dir unavailable: {e}"))
        })?;
        Ok(Self {
            path: dir.join(FILE_NAME),
        })
    }

    /// 容错读：文件缺失 / 解析失败一律降级为默认值（绝不返回 `Err`，
    /// 以便 `load_identity` 走 `Ok(None)` → core 生成新身份的路径）。
    async fn read(&self) -> DevIdentityFile {
        match fs::read_to_string(&self.path).await {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                warn!("[dev-identity] parse {} failed: {err}", self.path.display());
                DevIdentityFile::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => DevIdentityFile::default(),
            Err(err) => {
                warn!("[dev-identity] read {} failed: {err}", self.path.display());
                DevIdentityFile::default()
            }
        }
    }

    async fn write(&self, file: &DevIdentityFile) -> CoreResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| CoreError::Identity(format!("dev identity: create dir failed: {e}")))?;
        }
        let text = serde_json::to_string_pretty(file).map_err(CoreError::Serialization)?;
        fs::write(&self.path, text)
            .await
            .map_err(|e| CoreError::Identity(format!("dev identity: write failed: {e}")))?;
        // dev 安全：明文私钥文件仅 owner 可读写。
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

#[async_trait]
impl KeychainProvider for FileKeychainProvider {
    async fn load_identity(&self) -> CoreResult<Option<DeviceIdentityBytes>> {
        let file = self.read().await;
        if file.keypair.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DeviceIdentityBytes {
                keypair: file.keypair,
            }))
        }
    }

    async fn save_identity(&self, identity: DeviceIdentityBytes) -> CoreResult<()> {
        let mut file = self.read().await;
        file.keypair = identity.keypair;
        self.write(&file).await
    }

    async fn delete_identity(&self) -> CoreResult<()> {
        let mut file = self.read().await;
        file.keypair = Vec::new();
        self.write(&file).await
    }

    async fn load_migration_state(&self) -> CoreResult<IdentityMigrationState> {
        // dev 无 Stronghold→keychain 迁移概念，对齐 load_or_create_identity 首次生成即 Completed。
        Ok(IdentityMigrationState::Completed)
    }

    async fn save_migration_state(&self, state: IdentityMigrationState) -> CoreResult<()> {
        let mut file = self.read().await;
        file.migration_completed = matches!(state, IdentityMigrationState::Completed);
        self.write(&file).await
    }

    async fn load_paired_devices(&self) -> CoreResult<Vec<PairedDeviceInfo>> {
        Ok(self.read().await.paired_devices)
    }

    async fn save_paired_devices(&self, devices: Vec<PairedDeviceInfo>) -> CoreResult<()> {
        let mut file = self.read().await;
        file.paired_devices = devices;
        self.write(&file).await
    }
}
