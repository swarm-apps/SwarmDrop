//! Keychain bridge —— host 平台(iOS Keychain / Android EncryptedSharedPreferences)
//! 持久化身份密钥和配对设备清单。Rust 侧把 host 的 `ForeignKeychainProvider`
//! 适配成 core 的 `KeychainProvider` trait。

use std::sync::Arc;

use async_trait::async_trait;
use swarmdrop_core::device::PairedDeviceInfo;
use swarmdrop_core::host::{DeviceIdentityBytes, IdentityMigrationState, KeychainProvider};
use swarmdrop_core::{AppError, AppResult};

use crate::error::FfiError;

#[uniffi::export(with_foreign)]
#[async_trait]
pub trait ForeignKeychainProvider: Send + Sync {
    async fn load_identity(&self) -> Result<Option<Vec<u8>>, FfiError>;
    async fn save_identity(&self, keypair: Vec<u8>) -> Result<(), FfiError>;
    async fn delete_identity(&self) -> Result<(), FfiError>;
    async fn load_paired_devices_json(&self) -> Result<String, FfiError>;
    async fn save_paired_devices_json(&self, devices_json: String) -> Result<(), FfiError>;
}

pub(crate) struct MobileKeychainAdapter {
    foreign: Arc<dyn ForeignKeychainProvider>,
}

impl MobileKeychainAdapter {
    pub(crate) fn new(foreign: Arc<dyn ForeignKeychainProvider>) -> Self {
        Self { foreign }
    }
}

#[async_trait]
impl KeychainProvider for MobileKeychainAdapter {
    async fn load_identity(&self) -> AppResult<Option<DeviceIdentityBytes>> {
        self.foreign
            .load_identity()
            .await
            .map(|value| value.map(|keypair| DeviceIdentityBytes { keypair }))
            .map_err(Into::into)
    }

    async fn save_identity(&self, identity: DeviceIdentityBytes) -> AppResult<()> {
        self.foreign
            .save_identity(identity.keypair)
            .await
            .map_err(Into::into)
    }

    async fn delete_identity(&self) -> AppResult<()> {
        self.foreign.delete_identity().await.map_err(Into::into)
    }

    // mobile 没有 Stronghold → keychain 迁移路径,直接返回 Completed
    async fn load_migration_state(&self) -> AppResult<IdentityMigrationState> {
        Ok(IdentityMigrationState::Completed)
    }

    async fn save_migration_state(&self, _state: IdentityMigrationState) -> AppResult<()> {
        Ok(())
    }

    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>> {
        let value = self.foreign.load_paired_devices_json().await?;
        if value.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&value).map_err(AppError::Serialization)
    }

    async fn save_paired_devices(&self, devices: Vec<PairedDeviceInfo>) -> AppResult<()> {
        let value = serde_json::to_string(&devices).map_err(AppError::Serialization)?;
        self.foreign
            .save_paired_devices_json(value)
            .await
            .map_err(Into::into)
    }
}
