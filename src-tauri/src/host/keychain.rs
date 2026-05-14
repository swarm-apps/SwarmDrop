//! Desktop identity storage backed by the system keychain.

use async_trait::async_trait;
use keyring::{Entry, Error as KeyringError};
use swarmdrop_core::device::PairedDeviceInfo;
use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_core::host::{DeviceIdentityBytes, IdentityMigrationState, KeychainProvider};

const SERVICE: &str = "com.yexiyue.swarmdrop";
const IDENTITY_USER: &str = "device-identity";
const PAIRED_DEVICES_USER: &str = "paired-devices";
const MIGRATION_STATE_USER: &str = "identity-migration-state";

#[derive(Debug, Clone, Default)]
pub struct DesktopKeychainProvider;

impl DesktopKeychainProvider {
    pub fn new() -> CoreResult<Self> {
        Ok(Self)
    }
}

#[async_trait]
impl KeychainProvider for DesktopKeychainProvider {
    async fn load_identity(&self) -> CoreResult<Option<DeviceIdentityBytes>> {
        run_keyring(|| {
            optional_entry_secret(IDENTITY_USER)
                .map(|value| value.map(|keypair| DeviceIdentityBytes { keypair }))
        })
        .await
    }

    async fn save_identity(&self, identity: DeviceIdentityBytes) -> CoreResult<()> {
        run_keyring(move || {
            entry(IDENTITY_USER)?
                .set_secret(&identity.keypair)
                .map_err(map_keyring_error)
        })
        .await
    }

    async fn delete_identity(&self) -> CoreResult<()> {
        run_keyring(|| delete_entry_if_exists(IDENTITY_USER)).await
    }

    async fn load_migration_state(&self) -> CoreResult<IdentityMigrationState> {
        run_keyring(|| {
            let state = optional_entry_password(MIGRATION_STATE_USER)?;
            Ok(match state.as_deref() {
                Some("completed") => IdentityMigrationState::Completed,
                _ => IdentityMigrationState::NotStarted,
            })
        })
        .await
    }

    async fn save_migration_state(&self, state: IdentityMigrationState) -> CoreResult<()> {
        run_keyring(move || {
            let value = match state {
                IdentityMigrationState::NotStarted => "not_started",
                IdentityMigrationState::Completed => "completed",
            };
            entry(MIGRATION_STATE_USER)?
                .set_password(value)
                .map_err(map_keyring_error)
        })
        .await
    }

    async fn load_paired_devices(&self) -> CoreResult<Vec<PairedDeviceInfo>> {
        run_keyring(|| {
            let Some(value) = optional_entry_password(PAIRED_DEVICES_USER)? else {
                return Ok(Vec::new());
            };
            serde_json::from_str(&value).map_err(CoreError::Serialization)
        })
        .await
    }

    async fn save_paired_devices(&self, devices: Vec<PairedDeviceInfo>) -> CoreResult<()> {
        run_keyring(move || {
            let value = serde_json::to_string(&devices).map_err(CoreError::Serialization)?;
            entry(PAIRED_DEVICES_USER)?
                .set_password(&value)
                .map_err(map_keyring_error)
        })
        .await
    }
}

async fn run_keyring<F, T>(task: F) -> CoreResult<T>
where
    F: FnOnce() -> CoreResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(task).await?
}

fn entry(user: &str) -> CoreResult<Entry> {
    Entry::new(SERVICE, user).map_err(map_keyring_error)
}

fn optional_entry_secret(user: &str) -> CoreResult<Option<Vec<u8>>> {
    match entry(user)?.get_secret() {
        Ok(value) => Ok(Some(value)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(map_keyring_error(error)),
    }
}

fn optional_entry_password(user: &str) -> CoreResult<Option<String>> {
    match entry(user)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(map_keyring_error(error)),
    }
}

fn delete_entry_if_exists(user: &str) -> CoreResult<()> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(map_keyring_error(error)),
    }
}

fn map_keyring_error(error: KeyringError) -> CoreError {
    CoreError::Identity(format!("keychain error: {error}"))
}
