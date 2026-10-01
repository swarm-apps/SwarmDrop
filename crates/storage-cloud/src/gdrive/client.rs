//! Drive HTTP 与访问租约、重试和有界元数据响应。
use super::object::{DriveFile, DriveFiles, FILE_FIELDS, validate_id};
use crate::{CloudStorageError, StorageResult};
use reqwest::{Response, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use std::{sync::Arc, time::Duration};
use swarmdrop_cloud_auth::{CloudAccountManager, CloudAuthError};

const METADATA_LIMIT: usize = 1024 * 1024;
pub(super) struct DriveClient {
    accounts: Arc<CloudAccountManager>,
    pub(super) http: reqwest::Client,
}
impl DriveClient {
    pub(super) fn new(accounts: Arc<CloudAccountManager>) -> StorageResult<Self> {
        Ok(Self {
            accounts,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(60))
                .build()
                .map_err(|_| CloudStorageError::Configuration)?,
        })
    }
    pub(super) async fn request(
        &self,
        account: &str,
        request: reqwest::RequestBuilder,
    ) -> StorageResult<Response> {
        let mut token = self
            .accounts
            .access_token(account, Duration::from_secs(90))
            .await
            .map_err(auth_error)?;
        let mut refreshed = false;
        let mut attempt = 0;
        loop {
            let result = request
                .try_clone()
                .ok_or(CloudStorageError::Protocol)?
                .bearer_auth(token.secret())
                .send()
                .await;
            match result {
                Ok(response) if response.status() == StatusCode::UNAUTHORIZED => {
                    if refreshed {
                        self.accounts
                            .require_reconnect_after_rejection(account, &token)
                            .await
                            .map_err(auth_error)?;
                        return Err(CloudStorageError::ReconnectRequired);
                    }
                    token = self
                        .accounts
                        .refresh_after_rejection(account, &token)
                        .await
                        .map_err(auth_error)?;
                    refreshed = true;
                }
                Ok(response)
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    if attempt >= 3 {
                        return Err(CloudStorageError::Retryable);
                    }
                    let seconds = response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(1 << attempt)
                        .min(30);
                    tokio::time::sleep(Duration::from_secs(seconds)).await;
                    attempt += 1;
                }
                Ok(response) if response.status() == StatusCode::FORBIDDEN => {
                    let body: serde_json::Value = read_json(response).await?;
                    let limited = body
                        .pointer("/error/errors")
                        .and_then(|value| value.as_array())
                        .is_some_and(|errors| {
                            errors.iter().any(|error| {
                                matches!(
                                    error.get("reason").and_then(|value| value.as_str()),
                                    Some("rateLimitExceeded" | "userRateLimitExceeded")
                                )
                            })
                        });
                    if !limited {
                        return Err(CloudStorageError::Configuration);
                    }
                    if attempt >= 3 {
                        return Err(CloudStorageError::Retryable);
                    }
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                    attempt += 1;
                }
                Ok(response) => return Ok(response),
                Err(_) if attempt < 3 => {
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                    attempt += 1;
                }
                Err(_) => return Err(CloudStorageError::Retryable),
            }
        }
    }
    pub(super) async fn get_file(
        &self,
        account: &str,
        id: &str,
    ) -> StorageResult<Option<DriveFile>> {
        validate_id(id)?;
        let response = self
            .request(
                account,
                self.http
                    .get(format!("https://www.googleapis.com/drive/v3/files/{id}"))
                    .query(&[("fields", FILE_FIELDS)]),
            )
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        ensure_success(&response)?;
        let file: DriveFile = read_json(response).await?;
        Ok((!file.trashed).then_some(file))
    }
    pub(super) async fn find_files(
        &self,
        account: &str,
        query: String,
    ) -> StorageResult<Vec<DriveFile>> {
        let mut files = Vec::new();
        let mut page = String::new();
        loop {
            let response = self
                .request(
                    account,
                    self.http
                        .get("https://www.googleapis.com/drive/v3/files")
                        .query(&[
                            ("q", query.as_str()),
                            (
                                "fields",
                                "nextPageToken,files(id,name,size,parents,appProperties,trashed)",
                            ),
                            ("pageSize", "100"),
                            ("pageToken", page.as_str()),
                        ]),
                )
                .await?;
            ensure_success(&response)?;
            let result: DriveFiles = read_json(response).await?;
            files.extend(result.files);
            match result.next_page_token {
                Some(next) if files.len() < 1000 => page = next,
                Some(_) => return Err(CloudStorageError::Configuration),
                None => return Ok(files),
            }
        }
    }
    pub(super) async fn generate_id(&self, account: &str) -> StorageResult<String> {
        #[derive(Deserialize)]
        struct Ids {
            ids: Vec<String>,
        }
        let response = self
            .request(
                account,
                self.http
                    .get("https://www.googleapis.com/drive/v3/files/generateIds")
                    .query(&[("count", "1"), ("space", "drive"), ("type", "files")]),
            )
            .await?;
        ensure_success(&response)?;
        let result: Ids = read_json(response).await?;
        let id = result
            .ids
            .into_iter()
            .next()
            .ok_or(CloudStorageError::Protocol)?;
        validate_id(&id)?;
        Ok(id)
    }
}
fn auth_error(error: CloudAuthError) -> CloudStorageError {
    match error {
        CloudAuthError::AccountNotFound
        | CloudAuthError::ReconnectRequired
        | CloudAuthError::Denied => CloudStorageError::ReconnectRequired,
        CloudAuthError::SaveFailed | CloudAuthError::ReadFailed | CloudAuthError::DeleteFailed => {
            CloudStorageError::Checkpoint
        }
        CloudAuthError::InvalidConfiguration => CloudStorageError::Configuration,
        _ => CloudStorageError::Retryable,
    }
}
pub(super) async fn read_json<T: DeserializeOwned>(mut response: Response) -> StorageResult<T> {
    let mut data = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| CloudStorageError::Retryable)?
    {
        if data.len() + chunk.len() > METADATA_LIMIT {
            return Err(CloudStorageError::Protocol);
        }
        data.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&data).map_err(|_| CloudStorageError::Protocol)
}
pub(super) fn ensure_success(response: &Response) -> StorageResult<()> {
    if response.status().is_success() {
        Ok(())
    } else {
        Err(CloudStorageError::Configuration)
    }
}
