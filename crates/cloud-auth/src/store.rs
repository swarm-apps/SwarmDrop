use crate::{AccountCredentials, CloudAuthError, ProviderId};
use async_trait::async_trait;
use std::{io::Write, path::PathBuf};

#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn load_all(&self) -> Result<Vec<AccountCredentials>, CloudAuthError>;
    async fn save(&self, account: &AccountCredentials) -> Result<(), CloudAuthError>;
    async fn delete(&self, provider: ProviderId, id: &str) -> Result<(), CloudAuthError>;
}

pub struct JsonCredentialStore {
    root: PathBuf,
}
impl JsonCredentialStore {
    pub fn new(local_data_dir: PathBuf) -> Self {
        Self {
            root: local_data_dir.join("cloud-accounts"),
        }
    }
    fn path(&self, provider: ProviderId, id: &str) -> Result<PathBuf, CloudAuthError> {
        uuid::Uuid::parse_str(id).map_err(|_| CloudAuthError::InvalidConfiguration)?;
        Ok(self.root.join(provider.as_str()).join(format!("{id}.json")))
    }
}

#[async_trait]
impl CredentialStore for JsonCredentialStore {
    async fn load_all(&self) -> Result<Vec<AccountCredentials>, CloudAuthError> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let mut accounts = Vec::new();
            let directories = match std::fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(accounts),
                Err(_) => return Err(CloudAuthError::ReadFailed),
            };
            for directory in directories {
                let dir = directory.map_err(|_| CloudAuthError::ReadFailed)?.path();
                if !dir.is_dir() {
                    continue;
                }
                let entries = match std::fs::read_dir(&dir) {
                    Ok(entries) => entries,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => return Err(CloudAuthError::ReadFailed),
                };
                for entry in entries {
                    let path = entry.map_err(|_| CloudAuthError::ReadFailed)?.path();
                    if path.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    let data = std::fs::read(&path).map_err(|_| CloudAuthError::ReadFailed)?;
                    let account: AccountCredentials =
                        serde_json::from_slice(&data).map_err(|_| CloudAuthError::ReadFailed)?;
                    if dir.file_name().and_then(|name| name.to_str())
                        != Some(account.snapshot.provider.as_str())
                        || path.file_stem().and_then(|s| s.to_str()) != Some(&account.snapshot.id)
                        || uuid::Uuid::parse_str(&account.snapshot.id).is_err()
                    {
                        return Err(CloudAuthError::ReadFailed);
                    }
                    accounts.push(account);
                }
            }
            Ok(accounts)
        })
        .await
        .map_err(|_| CloudAuthError::ReadFailed)?
    }

    async fn save(&self, account: &AccountCredentials) -> Result<(), CloudAuthError> {
        let path = self.path(account.snapshot.provider, &account.snapshot.id)?;
        let prefix = format!(".{}-", account.snapshot.id);
        let data = serde_json::to_vec(account).map_err(|_| CloudAuthError::SaveFailed)?;
        tokio::task::spawn_blocking(move || {
            let dir = path.parent().ok_or(CloudAuthError::SaveFailed)?;
            std::fs::create_dir_all(dir).map_err(|_| CloudAuthError::SaveFailed)?;
            let mut tmp = tempfile::Builder::new()
                .prefix(&prefix)
                .tempfile_in(dir)
                .map_err(|_| CloudAuthError::SaveFailed)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tmp.as_file()
                    .set_permissions(std::fs::Permissions::from_mode(0o600))
                    .map_err(|_| CloudAuthError::SaveFailed)?;
            }
            tmp.write_all(&data)
                .and_then(|_| tmp.as_file().sync_all())
                .map_err(|_| CloudAuthError::SaveFailed)?;
            tmp.persist(&path).map_err(|_| CloudAuthError::SaveFailed)?;
            #[cfg(unix)]
            std::fs::File::open(dir)
                .and_then(|f| f.sync_all())
                .map_err(|_| CloudAuthError::SaveFailed)?;
            Ok(())
        })
        .await
        .map_err(|_| CloudAuthError::SaveFailed)?
    }

    async fn delete(&self, provider: ProviderId, id: &str) -> Result<(), CloudAuthError> {
        let path = self.path(provider, id)?;
        let prefix = format!(".{id}-");
        tokio::task::spawn_blocking(move || {
            let dir = path.parent().ok_or(CloudAuthError::DeleteFailed)?;
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(CloudAuthError::DeleteFailed),
            }
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(_) => return Err(CloudAuthError::DeleteFailed),
            };
            for entry in entries {
                let entry = entry.map_err(|_| CloudAuthError::DeleteFailed)?;
                if entry.file_name().to_string_lossy().starts_with(&prefix) {
                    std::fs::remove_file(entry.path()).map_err(|_| CloudAuthError::DeleteFailed)?;
                }
            }
            #[cfg(unix)]
            std::fs::File::open(dir)
                .and_then(|f| f.sync_all())
                .map_err(|_| CloudAuthError::DeleteFailed)?;
            Ok(())
        })
        .await
        .map_err(|_| CloudAuthError::DeleteFailed)?
    }
}
