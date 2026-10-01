//! 应用目录的归属核验、预分配 ID 与按账户串行创建。
use super::{
    checkpoint::{CheckpointStore, DirectoryCheckpoint},
    client::{DriveClient, ensure_success},
    object::{escape_query, validate_id},
};
use crate::persistence::{load_json, save_json};
use crate::{CloudStorageError, PublishIntent, StorageResult};
use reqwest::StatusCode;
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

pub(super) struct DirectoryTree {
    api: Arc<DriveClient>,
    checkpoints: CheckpointStore,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}
impl DirectoryTree {
    pub(super) fn new(api: Arc<DriveClient>, checkpoints: CheckpointStore) -> Self {
        Self {
            api,
            checkpoints,
            locks: Mutex::new(HashMap::new()),
        }
    }
    async fn directory(
        &self,
        account: &str,
        parent: &str,
        name: &str,
        key: &str,
    ) -> StorageResult<String> {
        let key = digest(&format!("{account}\0{parent}\0{key}"));
        let path = self.checkpoints.directory_path(&key);
        let existing: Option<DirectoryCheckpoint> = load_json(path.clone()).await?;
        let cached = existing.map(|entry| entry.id);
        if let Some(id) = &cached
            && let Some(file) = self.api.get_file(account, id).await?
        {
            if (parent == "root" || file.parents.iter().any(|id| id == parent))
                && file.app_properties.get("directory_key") == Some(&key)
            {
                return Ok(file.id);
            }
            // 用户移动目录或切换 OAuth 客户端后，不继续写入失去归属验证的目录。
            return Err(CloudStorageError::Configuration);
        }
        let found = self.api.find_files(account, format!("trashed = false and '{}' in parents and mimeType = 'application/vnd.google-apps.folder' and appProperties has {{ key='directory_key' and value='{}' }}", escape_query(parent), key)).await?;
        if let Some(file) = found.into_iter().next() {
            save_json(
                path,
                &DirectoryCheckpoint {
                    id: file.id.clone(),
                },
            )
            .await?;
            return Ok(file.id);
        }
        let mut id = match cached {
            Some(id) => id,
            None => self.api.generate_id(account).await?,
        };
        for attempt in 0..2 {
            save_json(path.clone(), &DirectoryCheckpoint { id: id.clone() }).await?;
            let response = self.api.request(account, self.api.http.post("https://www.googleapis.com/drive/v3/files").query(&[("fields", "id")]).json(&json!({"id": id,"name": name,"mimeType":"application/vnd.google-apps.folder","parents":[parent],"appProperties":{"directory_key":key}}))).await?;
            if response.status() != StatusCode::CONFLICT {
                ensure_success(&response)?;
                return Ok(id);
            }
            if let Some(file) = self.api.get_file(account, &id).await? {
                if file.app_properties.get("directory_key") != Some(&key)
                    || (parent != "root" && !file.parents.iter().any(|id| id == parent))
                {
                    return Err(CloudStorageError::Configuration);
                }
                return Ok(id);
            }
            // 已删除的预分配 ID 仍可能返回 409；查不到完成对象后才换 ID，避免重试制造重复目录。
            if attempt == 0 {
                id = self.api.generate_id(account).await?;
            }
        }
        Err(CloudStorageError::Retryable)
    }
    pub(super) async fn parent(
        &self,
        intent: &PublishIntent,
        parts: &[String],
    ) -> StorageResult<String> {
        let lock = self
            .locks
            .lock()
            .await
            .entry(intent.account_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let root = intent.root.as_deref().unwrap_or("root");
        validate_id(root)?;
        let app_root = self
            .directory(&intent.account_id, root, "SwarmDrop", "SwarmDrop")
            .await?;
        let receiver = &intent.identity.receiver_device_id;
        validate_id(receiver)?;
        let mut parent = self
            .directory(&intent.account_id, &app_root, receiver, receiver)
            .await?;
        let mut key = receiver.clone();
        for part in parts {
            key.push('/');
            key.push_str(part);
            parent = self
                .directory(&intent.account_id, &parent, part, &key)
                .await?;
        }
        Ok(parent)
    }
}
fn digest(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex().to_string()
}
