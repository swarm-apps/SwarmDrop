//! Google resumable 的服务端偏移、过期重建与完成对账。
use super::{
    GoogleDrivePublisher,
    checkpoint::UploadCheckpoint,
    client::{ensure_success, read_json},
    object::{DriveFile, FILE_FIELDS, object_ref, properties, verify_file},
};
use crate::{CloudStorageError, ProgressSink, PublishIntent, PublishProgress, StorageResult};
use bytes::Bytes;
use reqwest::{Method, Response, StatusCode};
use serde_json::json;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use swarmdrop_host::CloudObjectRef;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
const CHUNK_SIZE: usize = 8 * 1024 * 1024;

enum UploadState {
    Pending(u64),
    Complete(DriveFile),
    Expired,
}

impl GoogleDrivePublisher {
    async fn initialize(&self, checkpoint: &mut UploadCheckpoint) -> StorageResult<()> {
        let intent = &checkpoint.intent;
        if intent.size == 0 {
            let response = self.api.request(&intent.account_id, self.api.http.post("https://www.googleapis.com/drive/v3/files").query(&[("fields", FILE_FIELDS)]).json(&json!({"id":checkpoint.object_id,"name":checkpoint.remote_name,"mimeType":"application/octet-stream","parents":[checkpoint.parent_id],"appProperties":properties(intent)?}))).await?;
            let file = if response.status() == StatusCode::CONFLICT {
                self.api
                    .get_file(&intent.account_id, &checkpoint.object_id)
                    .await?
                    .ok_or(CloudStorageError::Retryable)?
            } else {
                ensure_success(&response)?;
                read_json(response).await?
            };
            verify_file(&file, intent, &checkpoint.parent_id)?;
            checkpoint.completed = Some(object_ref(intent, file));
            return self.checkpoints.save(checkpoint).await;
        }
        let response = self.api.request(&intent.account_id, self.api.http.post("https://www.googleapis.com/upload/drive/v3/files").query(&[("uploadType", "resumable"), ("fields", FILE_FIELDS)]).header("X-Upload-Content-Type", "application/octet-stream").header("X-Upload-Content-Length", intent.size).json(&json!({"id":checkpoint.object_id,"name":checkpoint.remote_name,"parents":[checkpoint.parent_id],"appProperties":properties(intent)?}))).await?;
        if response.status() == StatusCode::CONFLICT {
            let file = self
                .api
                .get_file(&intent.account_id, &checkpoint.object_id)
                .await?
                .ok_or(CloudStorageError::Retryable)?;
            verify_file(&file, intent, &checkpoint.parent_id)?;
            checkpoint.completed = Some(object_ref(intent, file));
        } else {
            ensure_success(&response)?;
            let uri = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or(CloudStorageError::Protocol)?;
            validate_session(uri)?;
            checkpoint.session_uri = Some(uri.to_owned());
            checkpoint.offset = 0;
            checkpoint.created_at = now_seconds();
        }
        self.checkpoints.save(checkpoint).await
    }
    async fn state(&self, checkpoint: &UploadCheckpoint) -> StorageResult<UploadState> {
        let uri = checkpoint
            .session_uri
            .as_deref()
            .ok_or(CloudStorageError::Protocol)?;
        validate_session(uri)?;
        let response = self
            .api
            .request(
                &checkpoint.intent.account_id,
                self.api
                    .http
                    .request(Method::PUT, uri)
                    .header("Content-Length", "0")
                    .header(
                        "Content-Range",
                        format!("bytes */{}", checkpoint.intent.size),
                    )
                    .body(Bytes::new()),
            )
            .await?;
        upload_state(response, checkpoint.intent.size).await
    }
    pub(super) async fn reconcile(&self, checkpoint: &mut UploadCheckpoint) -> StorageResult<bool> {
        if let Some(file) = self
            .api
            .get_file(&checkpoint.intent.account_id, &checkpoint.object_id)
            .await?
        {
            verify_file(&file, &checkpoint.intent, &checkpoint.parent_id)?;
            checkpoint.completed = Some(object_ref(&checkpoint.intent, file));
            self.checkpoints.save(checkpoint).await?;
            return Ok(true);
        }
        Ok(false)
    }
}
impl GoogleDrivePublisher {
    pub(super) async fn upload(
        &self,
        mut file: tokio::fs::File,
        mut checkpoint: UploadCheckpoint,
        progress: ProgressSink,
    ) -> StorageResult<CloudObjectRef> {
        let mut resets = 0;
        let started = Instant::now();
        let mut sent = 0u64;
        let mut query_state = true;
        if checkpoint.session_uri.is_none() {
            self.initialize(&mut checkpoint).await?;
        }
        loop {
            if let Some(object) = &checkpoint.completed {
                progress(progress_value(
                    &checkpoint.intent,
                    checkpoint.intent.size,
                    (sent as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64,
                ));
                return Ok(object.clone());
            }
            if query_state {
                match self.state(&checkpoint).await? {
                    UploadState::Complete(remote) => {
                        verify_file(&remote, &checkpoint.intent, &checkpoint.parent_id)?;
                        checkpoint.completed = Some(object_ref(&checkpoint.intent, remote));
                        self.checkpoints.save(&checkpoint).await?;
                        continue;
                    }
                    UploadState::Expired => {
                        if self.reconcile(&mut checkpoint).await? {
                            continue;
                        }
                        if resets >= 2 {
                            return Err(CloudStorageError::Retryable);
                        }
                        resets += 1;
                        checkpoint.session_uri = None;
                        self.checkpoints.save(&checkpoint).await?;
                        self.initialize(&mut checkpoint).await?;
                        continue;
                    }
                    UploadState::Pending(offset) => checkpoint.offset = offset,
                }
                query_state = false;
                progress(progress_value(&checkpoint.intent, checkpoint.offset, 0));
            }
            self.checkpoints.save(&checkpoint).await?;
            let offset = checkpoint.offset;
            let total = checkpoint.intent.size;
            if offset == total && total != 0 {
                return Err(CloudStorageError::Retryable);
            }
            let length = (total - offset).min(CHUNK_SIZE as u64) as usize;
            let mut data = vec![0; length];
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|_| CloudStorageError::StagingChanged)?;
            file.read_exact(&mut data)
                .await
                .map_err(|_| CloudStorageError::StagingChanged)?;
            let uri = checkpoint
                .session_uri
                .as_deref()
                .ok_or(CloudStorageError::Protocol)?;
            validate_session(uri)?;
            let range = if total == 0 {
                "bytes */0".into()
            } else {
                format!("bytes {}-{}/{}", offset, offset + length as u64 - 1, total)
            };
            let response = self
                .api
                .request(
                    &checkpoint.intent.account_id,
                    self.api
                        .http
                        .request(Method::PUT, uri)
                        .header("Content-Type", "application/octet-stream")
                        .header("Content-Length", length)
                        .header("Content-Range", range)
                        .body(Bytes::from(data)),
                )
                .await;
            // 网络结果不确定时下轮先查偏移；不能按「本机发了多少」前移检查点。
            match response {
                Ok(response) => match upload_state(response, total).await? {
                    UploadState::Complete(remote) => {
                        verify_file(&remote, &checkpoint.intent, &checkpoint.parent_id)?;
                        checkpoint.completed = Some(object_ref(&checkpoint.intent, remote));
                        checkpoint.offset = total;
                    }
                    UploadState::Pending(next) if next > offset => checkpoint.offset = next,
                    UploadState::Pending(_) => return Err(CloudStorageError::Retryable),
                    UploadState::Expired => {
                        if !self.reconcile(&mut checkpoint).await? {
                            if resets >= 2 {
                                return Err(CloudStorageError::Retryable);
                            }
                            resets += 1;
                            checkpoint.session_uri = None;
                            self.checkpoints.save(&checkpoint).await?;
                            self.initialize(&mut checkpoint).await?;
                            query_state = true;
                        }
                    }
                },
                Err(CloudStorageError::Retryable) => {
                    if !self.reconcile(&mut checkpoint).await? {
                        return Err(CloudStorageError::Retryable);
                    }
                }
                Err(error) => return Err(error),
            }
            self.checkpoints.save(&checkpoint).await?;
            sent += checkpoint.offset.saturating_sub(offset);
            let rate = (sent as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
            progress(progress_value(&checkpoint.intent, checkpoint.offset, rate));
        }
    }
}
async fn upload_state(response: Response, total: u64) -> StorageResult<UploadState> {
    match response.status().as_u16() {
        200 | 201 => Ok(UploadState::Complete(read_json(response).await?)),
        404 | 410 => Ok(UploadState::Expired),
        308 => {
            let offset = match response.headers().get("range") {
                None => 0,
                Some(range) => range
                    .to_str()
                    .ok()
                    .and_then(|value| value.strip_prefix("bytes=0-"))
                    .and_then(|end| end.parse::<u64>().ok())
                    .and_then(|end| end.checked_add(1))
                    .ok_or(CloudStorageError::Protocol)?,
            };
            if offset > total {
                return Err(CloudStorageError::Protocol);
            }
            Ok(UploadState::Pending(offset))
        }
        _ => Err(CloudStorageError::Configuration),
    }
}
fn validate_session(uri: &str) -> StorageResult<()> {
    let url = url::Url::parse(uri).map_err(|_| CloudStorageError::Protocol)?;
    if url.scheme() != "https"
        || url.host_str() != Some("www.googleapis.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().starts_with("/upload/drive/v3/files")
    {
        return Err(CloudStorageError::Protocol);
    }
    Ok(())
}
pub(super) fn progress_value(
    intent: &PublishIntent,
    uploaded_bytes: u64,
    bytes_per_second: u64,
) -> PublishProgress {
    PublishProgress {
        session_id: intent.identity.session_id.clone(),
        file_id: intent.identity.file_id,
        uploaded_bytes,
        total_bytes: intent.size,
        bytes_per_second,
        failure: None,
    }
}
pub(super) fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
