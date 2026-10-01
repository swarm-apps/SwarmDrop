use crate::persistence::{load_json, remove_file, save_json};
use crate::{CloudStorageError, PublishIntent, StagedFile, StorageResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use swarmdrop_host::CloudObjectRef;

// session_uri 是能力 URL：刻意不派生 Debug，也不把该类型导出给宿主。
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct UploadCheckpoint {
    pub intent: PublishIntent,
    pub staging_path: PathBuf,
    pub object_id: String,
    pub parent_id: String,
    pub remote_name: String,
    pub session_uri: Option<String>,
    pub offset: u64,
    pub created_at: u64,
    pub completed: Option<CloudObjectRef>,
}
#[derive(Serialize, Deserialize)]
pub(super) struct DirectoryCheckpoint {
    pub id: String,
}

#[derive(Clone)]
pub(super) struct CheckpointStore {
    root: PathBuf,
}
impl CheckpointStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            root: data_dir.join("cloud-uploads"),
        }
    }
    pub fn upload_path(&self, intent: &PublishIntent) -> PathBuf {
        self.root
            .join(digest(&intent.account_id))
            .join(format!("{}.json", intent.receipt_key()))
    }
    pub fn directory_path(&self, key: &str) -> PathBuf {
        self.root.join("directories").join(format!("{key}.json"))
    }
    pub async fn load(&self, intent: &PublishIntent) -> StorageResult<Option<UploadCheckpoint>> {
        load_json(self.upload_path(intent)).await
    }
    pub async fn save(&self, checkpoint: &UploadCheckpoint) -> StorageResult<()> {
        save_json(self.upload_path(&checkpoint.intent), checkpoint).await
    }
    pub async fn remove(&self, intent: &PublishIntent) -> StorageResult<()> {
        remove_file(self.upload_path(intent)).await
    }
    pub async fn remove_expired(
        &self,
        account: &str,
        session: &str,
        file_id: u32,
    ) -> StorageResult<()> {
        let dir = self.root.join(digest(account));
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(CloudStorageError::Checkpoint),
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|_| CloudStorageError::Checkpoint)?
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let checkpoint: Option<UploadCheckpoint> = load_json(path.clone()).await?;
            if let Some(checkpoint) = checkpoint {
                let intent = &checkpoint.intent;
                if intent.account_id == account
                    && intent.identity.session_id == session
                    && intent.identity.file_id == file_id
                {
                    if path != self.upload_path(intent) {
                        return Err(CloudStorageError::Checkpoint);
                    }
                    remove_file(path).await?;
                }
            }
        }
        Ok(())
    }
    pub async fn completed(&self) -> StorageResult<Vec<crate::PublishReceipt>> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let mut completed = Vec::new();
            if !root.exists() {
                return Ok(completed);
            }
            for dir in std::fs::read_dir(root).map_err(|_| CloudStorageError::Checkpoint)? {
                let dir = dir.map_err(|_| CloudStorageError::Checkpoint)?.path();
                if !dir.is_dir() || dir.file_name().is_some_and(|name| name == "directories") {
                    continue;
                }
                for entry in std::fs::read_dir(dir).map_err(|_| CloudStorageError::Checkpoint)? {
                    let path = entry.map_err(|_| CloudStorageError::Checkpoint)?.path();
                    if path.extension().is_none_or(|ext| ext != "json") {
                        continue;
                    }
                    let data = std::fs::read(path).map_err(|_| CloudStorageError::Checkpoint)?;
                    let checkpoint: UploadCheckpoint =
                        serde_json::from_slice(&data).map_err(|_| CloudStorageError::Checkpoint)?;
                    if let Some(object) = checkpoint.completed {
                        completed.push(crate::PublishReceipt {
                            intent: checkpoint.intent,
                            staged: StagedFile {
                                path: checkpoint.staging_path,
                            },
                            object,
                        });
                    }
                }
            }
            Ok(completed)
        })
        .await
        .map_err(|_| CloudStorageError::Checkpoint)?
    }
}

fn digest(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex().to_string()
}
