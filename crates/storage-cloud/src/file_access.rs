use crate::{CloudPublisher, CloudStorageError, ProgressSink, PublishIntent, StagedFile};
use async_trait::async_trait;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use swarmdrop_host::{
    AppError, AppResult, CoreSaveLocation, FileAccess, FileSinkId, FileSourceId, FinalizedSink,
    HostFileMetadata,
};
use swarmdrop_host_fs::LocalFileAccess;
use tokio::sync::Mutex;

struct CloudSink {
    local: Arc<LocalFileAccess>,
    sink: FileSinkId,
    staged: StagedFile,
    intent: PublishIntent,
}

pub struct CloudFileAccess {
    local: LocalFileAccess,
    staging_root: PathBuf,
    publisher: Arc<dyn CloudPublisher>,
    progress: ProgressSink,
    sinks: Mutex<HashMap<FileSinkId, Arc<CloudSink>>>,
}
impl CloudFileAccess {
    pub fn new(
        data_dir: PathBuf,
        publisher: Arc<dyn CloudPublisher>,
        progress: ProgressSink,
    ) -> Self {
        Self {
            local: LocalFileAccess::new(),
            staging_root: data_dir.join("cloud-staging"),
            publisher,
            progress,
            sinks: Mutex::new(HashMap::new()),
        }
    }
    async fn cloud_sink(&self, metadata: HostFileMetadata, resume: bool) -> AppResult<FileSinkId> {
        let Some(CoreSaveLocation::Cloud {
            provider,
            account_id,
            root,
        }) = metadata.save_dir.as_ref()
        else {
            return Err(AppError::Transfer("云目的地缺失".into()));
        };
        let identity = metadata
            .receive_identity
            .clone()
            .ok_or_else(|| AppError::Transfer("云暂存缺少接收身份".into()))?;
        let session = uuid::Uuid::parse_str(&identity.session_id)
            .map_err(|_| AppError::Transfer("云暂存接收身份无效".into()))?;
        uuid::Uuid::parse_str(account_id)
            .map_err(|_| AppError::Transfer("云账户标识无效".into()))?;
        let sink_id = FileSinkId(format!("cloud:{session}:{}", identity.file_id));
        let dir = self
            .staging_root
            .join(session.to_string())
            .join(identity.file_id.to_string());
        let staged = StagedFile {
            path: dir.join("payload.part"),
        };
        let intent = PublishIntent {
            provider: *provider,
            account_id: account_id.clone(),
            root: root.clone(),
            identity,
            relative_path: metadata.relative_path.clone(),
            original_name: metadata.name.clone(),
            size: metadata.size,
            checksum: metadata
                .checksum
                .clone()
                .ok_or_else(|| AppError::Transfer("云暂存缺少校验值".into()))?,
        };
        let mut sinks = self.sinks.lock().await;
        if resume && let Some(existing) = sinks.get(&sink_id) {
            if existing.intent != intent {
                return Err(AppError::Transfer("云接收目的地已变化".into()));
            }
            return Ok(sink_id);
        }
        // 每条接收拥有自己的本地端口，避免同名文件在不同会话中共用 active_sinks 键。
        let local = Arc::new(LocalFileAccess::new());
        let local_metadata = HostFileMetadata {
            relative_path: "payload".into(),
            save_dir: Some(CoreSaveLocation::Path {
                path: dir.to_string_lossy().into_owned(),
            }),
            ..metadata
        };
        let sink = if resume {
            local.open_or_create_sink(local_metadata).await?
        } else {
            local.create_sink(local_metadata).await?
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&staged.path, std::fs::Permissions::from_mode(0o600))
                .await?;
        }
        sinks.insert(
            sink_id.clone(),
            Arc::new(CloudSink {
                local,
                sink,
                staged,
                intent,
            }),
        );
        Ok(sink_id)
    }
    async fn lookup(&self, sink: &FileSinkId) -> Option<Arc<CloudSink>> {
        self.sinks.lock().await.get(sink).cloned()
    }
    /// 由宿主提供账本查询；恢复顺序与暂存安全检查由适配器统一负责。
    pub async fn reconcile_committed<F, Fut>(&self, is_committed: F) -> AppResult<()>
    where
        F: Fn(uuid::Uuid, u32, swarmdrop_host::CloudObjectRef) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = AppResult<bool>> + Send,
    {
        let receipts = self.publisher.completed_receipts().await.map_err(to_host)?;
        let mut first_error = None;
        for receipt in receipts {
            let result = async {
                let session = uuid::Uuid::parse_str(&receipt.intent.identity.session_id)
                    .map_err(|_| to_host(CloudStorageError::Checkpoint))?;
                if is_committed(
                    session,
                    receipt.intent.identity.file_id,
                    receipt.object.clone(),
                )
                .await?
                {
                    self.cleanup_committed(&receipt.intent, &receipt.staged)
                        .await?;
                }
                Ok::<_, AppError>(())
            }
            .await;
            if let Err(error) = result {
                // 一份回执损坏或 IO 失败不能阻止其他已记账文件回收。
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    /// 重启后只有 SQL 已记账的 receipt 可清理，目录必须仍位于当前应用暂存根内。
    async fn cleanup_committed(
        &self,
        intent: &PublishIntent,
        staged: &StagedFile,
    ) -> AppResult<()> {
        let session = uuid::Uuid::parse_str(&intent.identity.session_id)
            .map_err(|_| to_host(CloudStorageError::Checkpoint))?;
        let expected = self
            .staging_root
            .join(session.to_string())
            .join(intent.identity.file_id.to_string())
            .join("payload.part");
        if staged.path != expected {
            return Err(to_host(CloudStorageError::Checkpoint));
        }
        crate::persistence::remove_file(expected)
            .await
            .map_err(to_host)?;
        self.publisher
            .confirm_committed(intent)
            .await
            .map_err(to_host)
    }
}
#[async_trait]
impl FileAccess for CloudFileAccess {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata> {
        self.local.source_metadata(source).await
    }
    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>> {
        self.local.read_source_chunk(source, offset, length).await
    }
    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        if matches!(metadata.save_dir, Some(CoreSaveLocation::Cloud { .. })) {
            self.cloud_sink(metadata, false).await
        } else {
            self.local.create_sink(metadata).await
        }
    }
    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        if matches!(metadata.save_dir, Some(CoreSaveLocation::Cloud { .. })) {
            self.cloud_sink(metadata, true).await
        } else {
            self.local.open_or_create_sink(metadata).await
        }
    }
    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()> {
        match self.lookup(sink).await {
            Some(entry) => {
                entry
                    .local
                    .write_sink_chunk(&entry.sink, offset, data)
                    .await
            }
            None => self.local.write_sink_chunk(sink, offset, data).await,
        }
    }
    async fn sync_staged_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        let entry = self
            .lookup(sink)
            .await
            .ok_or_else(|| to_host(CloudStorageError::Checkpoint))?;
        tokio::fs::OpenOptions::new()
            .write(true)
            .open(&entry.staged.path)
            .await?
            .sync_all()
            .await?;
        Ok(())
    }
    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<FinalizedSink> {
        match self.lookup(sink).await {
            Some(entry) => {
                tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(&entry.staged.path)
                    .await?
                    .sync_all()
                    .await?;
                let object = match self
                    .publisher
                    .publish(
                        entry.staged.clone(),
                        entry.intent.clone(),
                        self.progress.clone(),
                    )
                    .await
                {
                    Ok(object) => object,
                    Err(error) => {
                        (self.progress)(crate::PublishProgress {
                            session_id: entry.intent.identity.session_id.clone(),
                            file_id: entry.intent.identity.file_id,
                            uploaded_bytes: 0,
                            total_bytes: entry.intent.size,
                            bytes_per_second: 0,
                            failure: Some(error.clone()),
                        });
                        return Err(to_host(error));
                    }
                };
                Ok(FinalizedSink::Cloud { object })
            }
            None => self.local.finalize_sink(sink).await,
        }
    }
    async fn confirm_sink_committed(&self, sink: &FileSinkId) -> AppResult<()> {
        if let Some(entry) = self.lookup(sink).await {
            entry.local.cleanup_sink(&entry.sink).await?;
            self.cleanup_committed(&entry.intent, &entry.staged).await?;
            self.sinks.lock().await.remove(sink);
        }
        Ok(())
    }
    async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        match self.lookup(sink).await {
            Some(entry) => {
                entry.local.cleanup_sink(&entry.sink).await?;
                crate::persistence::remove_file(entry.staged.path.clone())
                    .await
                    .map_err(to_host)?;
                self.publisher
                    .abandon(
                        &entry.intent.account_id,
                        &entry.intent.identity.session_id,
                        entry.intent.identity.file_id,
                    )
                    .await
                    .map_err(to_host)?;
                self.sinks.lock().await.remove(sink);
                Ok(())
            }
            None => self.local.cleanup_sink(sink).await,
        }
    }
    async fn cleanup_expired_sink(&self, metadata: HostFileMetadata) -> AppResult<()> {
        let Some(CoreSaveLocation::Cloud { account_id, .. }) = metadata.save_dir else {
            return self.local.cleanup_expired_sink(metadata).await;
        };
        let identity = metadata
            .receive_identity
            .ok_or_else(|| to_host(CloudStorageError::Checkpoint))?;
        let session = uuid::Uuid::parse_str(&identity.session_id)
            .map_err(|_| to_host(CloudStorageError::Checkpoint))?;
        let sink = FileSinkId(format!("cloud:{session}:{}", identity.file_id));
        if self.lookup(&sink).await.is_some() {
            return self.cleanup_sink(&sink).await;
        }
        let path = self
            .staging_root
            .join(session.to_string())
            .join(identity.file_id.to_string())
            .join("payload.part");
        crate::persistence::remove_file(path)
            .await
            .map_err(to_host)?;
        self.publisher
            .abandon(&account_id, &identity.session_id, identity.file_id)
            .await
            .map_err(to_host)
    }
    async fn delete_finalized_file(&self, uri: &str) -> AppResult<()> {
        self.local.delete_finalized_file(uri).await
    }
}
fn to_host(error: impl std::fmt::Display) -> AppError {
    AppError::Transfer(error.to_string())
}
