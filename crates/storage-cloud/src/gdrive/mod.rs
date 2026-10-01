//! Google Drive 发布协调；HTTP、目录和上传协议各有自己的实现边界。
mod checkpoint;
mod client;
mod directory;
mod object;
mod upload;

use crate::{
    CloudPublisher, CloudStorageError, ProgressSink, PublishIntent, StagedFile, StorageResult,
    staging::verify_staging,
};
use async_trait::async_trait;
use checkpoint::{CheckpointStore, UploadCheckpoint};
use client::DriveClient;
use directory::DirectoryTree;
use object::{clean_path, conflict_name, escape_query, object_ref, verify_file};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Weak},
};
use swarmdrop_cloud_auth::CloudAccountManager;
use swarmdrop_host::{CloudObjectRef, CloudProvider};
use tokio::{
    io::AsyncSeekExt,
    sync::{Mutex, Semaphore},
};
use upload::{now_seconds, progress_value};

pub struct GoogleDrivePublisher {
    api: Arc<DriveClient>,
    checkpoints: CheckpointStore,
    directories: DirectoryTree,
    // 两条上传各持有一个 8 MiB Bytes；重试复用同一缓冲，禁止按文件大小分配。
    uploads: Semaphore,
    publication_locks: Mutex<HashMap<String, Weak<Mutex<()>>>>,
}
impl GoogleDrivePublisher {
    pub fn new(accounts: Arc<CloudAccountManager>, data_dir: PathBuf) -> StorageResult<Self> {
        let api = Arc::new(DriveClient::new(accounts)?);
        let checkpoints = CheckpointStore::new(data_dir);
        Ok(Self {
            directories: DirectoryTree::new(api.clone(), checkpoints.clone()),
            api,
            checkpoints,
            uploads: Semaphore::new(2),
            publication_locks: Mutex::new(HashMap::new()),
        })
    }
}
impl GoogleDrivePublisher {
    async fn publish_file(
        &self,
        staged: StagedFile,
        intent: PublishIntent,
        progress: ProgressSink,
    ) -> StorageResult<CloudObjectRef> {
        if intent.provider != CloudProvider::GoogleDrive {
            return Err(CloudStorageError::Configuration);
        }
        let _permit = self
            .uploads
            .acquire()
            .await
            .map_err(|_| CloudStorageError::Retryable)?;
        // 同一路径的查重与发布必须串行，否则两个接收会在远端可见之前同时决定「没有同名文件」。
        let key = format!("{}:{}", intent.account_id, intent.path_key());
        let lock = {
            let mut locks = self.publication_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            match locks.get(&key).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(Mutex::new(()));
                    locks.insert(key, Arc::downgrade(&lock));
                    lock
                }
            }
        };
        let _publication = lock.lock().await;
        let mut parts = clean_path(&intent.relative_path)?;
        let name = parts.pop().ok_or(CloudStorageError::InvalidPath)?;
        if intent.identity.receiver_device_id.is_empty()
            || intent.identity.sender_device_id.is_empty()
            || uuid::Uuid::parse_str(&intent.identity.session_id).is_err()
        {
            return Err(CloudStorageError::Configuration);
        }
        let mut file = verify_staging(&staged, &intent).await?;
        let parent_id = self.directories.parent(&intent, &parts).await?;
        let mut checkpoint = if let Some(checkpoint) = self.checkpoints.load(&intent).await? {
            if checkpoint.intent != intent
                || checkpoint.staging_path != staged.path
                || checkpoint.parent_id != parent_id
            {
                return Err(CloudStorageError::StagingChanged);
            }
            checkpoint
        } else {
            let same_path = self.api.find_files(&intent.account_id, format!("trashed = false and '{}' in parents and appProperties has {{ key='target_path' and value='{}' }}", escape_query(&parent_id), intent.path_key())).await?;
            let same = same_path
                .into_iter()
                .find(|file| verify_file(file, &intent, &parent_id).is_ok());
            let (object_id, completed) = match same {
                Some(file) => (file.id.clone(), Some(object_ref(&intent, file))),
                None => {
                    let collisions = self
                        .api
                        .find_files(
                            &intent.account_id,
                            format!(
                                "trashed = false and '{}' in parents and name = '{}'",
                                escape_query(&parent_id),
                                escape_query(&name)
                            ),
                        )
                        .await?;
                    let id = self.api.generate_id(&intent.account_id).await?;
                    let completed = None;
                    // 重名只生成后缀，不覆盖；接收记录键的片段比本地递增数字在并发时更可靠。
                    let remote_name = if collisions.is_empty() {
                        name.clone()
                    } else {
                        conflict_name(&name, &intent.receipt_key()[..12])
                    };
                    let checkpoint = UploadCheckpoint {
                        intent: intent.clone(),
                        staging_path: staged.path.clone(),
                        object_id: id,
                        parent_id,
                        remote_name,
                        session_uri: None,
                        offset: 0,
                        created_at: now_seconds(),
                        completed,
                    };
                    self.checkpoints.save(&checkpoint).await?;
                    return self.upload(file, checkpoint, progress).await;
                }
            };
            let checkpoint = UploadCheckpoint {
                intent: intent.clone(),
                staging_path: staged.path,
                object_id,
                parent_id,
                remote_name: name,
                session_uri: None,
                offset: intent.size,
                created_at: now_seconds(),
                completed,
            };
            self.checkpoints.save(&checkpoint).await?;
            checkpoint
        };
        if self.reconcile(&mut checkpoint).await? {
            progress(progress_value(&intent, intent.size, 0));
            return checkpoint
                .completed
                .take()
                .ok_or(CloudStorageError::Protocol);
        }
        // 本机标记完成而对象已删除：不假装完成，也不自动复活用户删除的远端对象。
        if checkpoint.completed.is_some() {
            return Err(CloudStorageError::ObjectUnavailable);
        }
        file.seek(std::io::SeekFrom::Start(0))
            .await
            .map_err(|_| CloudStorageError::StagingChanged)?;
        self.upload(file, checkpoint, progress).await
    }
    async fn object_url(&self, object: &CloudObjectRef) -> StorageResult<url::Url> {
        if object.provider != CloudProvider::GoogleDrive {
            return Err(CloudStorageError::Configuration);
        }
        self.api
            .get_file(&object.account_id, &object.object_id)
            .await?
            .ok_or(CloudStorageError::ObjectUnavailable)?;
        url::Url::parse(&format!(
            "https://drive.google.com/file/d/{}/view",
            object.object_id
        ))
        .map_err(|_| CloudStorageError::Protocol)
    }
}

#[async_trait]
impl CloudPublisher for GoogleDrivePublisher {
    async fn publish(
        &self,
        staged: StagedFile,
        intent: PublishIntent,
        progress: ProgressSink,
    ) -> crate::CloudResult<CloudObjectRef> {
        let account = intent.account_id.clone();
        self.publish_file(staged, intent, progress)
            .await
            .map_err(|kind| {
                crate::CloudFailure::new(
                    kind,
                    CloudProvider::GoogleDrive,
                    Some(&account),
                    crate::CloudOperation::Publish,
                )
            })
    }
    async fn open_url(&self, object: &CloudObjectRef) -> crate::CloudResult<url::Url> {
        self.object_url(object).await.map_err(|kind| {
            crate::CloudFailure::new(
                kind,
                CloudProvider::GoogleDrive,
                Some(&object.account_id),
                crate::CloudOperation::Open,
            )
        })
    }
    async fn confirm_committed(&self, intent: &PublishIntent) -> crate::CloudResult<()> {
        self.checkpoints.remove(intent).await.map_err(|kind| {
            crate::CloudFailure::new(
                kind,
                CloudProvider::GoogleDrive,
                Some(&intent.account_id),
                crate::CloudOperation::Confirm,
            )
        })
    }
    async fn abandon(&self, account: &str, session: &str, file_id: u32) -> crate::CloudResult<()> {
        self.checkpoints
            .remove_expired(account, session, file_id)
            .await
            .map_err(|kind| {
                crate::CloudFailure::new(
                    kind,
                    CloudProvider::GoogleDrive,
                    Some(account),
                    crate::CloudOperation::Abandon,
                )
            })
    }
    async fn completed_receipts(&self) -> crate::CloudResult<Vec<crate::PublishReceipt>> {
        self.checkpoints.completed().await.map_err(|kind| {
            crate::CloudFailure::new(
                kind,
                CloudProvider::GoogleDrive,
                None,
                crate::CloudOperation::Restore,
            )
        })
    }
}
