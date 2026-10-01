//! 厂商无关的文件发布用例与恢复记录。
use crate::{CloudFailure, CloudResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use swarmdrop_host::{CloudObjectRef, CloudProvider, ReceiveFileIdentity};

/// 路径只在原生进程内流转，不能进入 IPC 或传输协议。
#[derive(Clone)]
pub struct StagedFile {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishIntent {
    pub provider: CloudProvider,
    pub account_id: String,
    pub root: Option<String>,
    pub identity: ReceiveFileIdentity,
    pub relative_path: String,
    pub original_name: String,
    pub size: u64,
    pub checksum: String,
}
impl PublishIntent {
    pub(crate) fn receipt_key(&self) -> String {
        blake3::hash(
            format!(
                "{}\0{}\0{}",
                self.identity.receiver_device_id, self.identity.session_id, self.identity.file_id
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string()
    }
    pub(crate) fn path_key(&self) -> String {
        blake3::hash(
            format!(
                "{}\0{}\0{}",
                self.root.as_deref().unwrap_or("root"),
                self.identity.receiver_device_id,
                self.relative_path
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PublishProgress {
    pub session_id: String,
    pub file_id: u32,
    pub uploaded_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_second: u64,
    pub failure: Option<CloudFailure>,
}
pub type ProgressSink = Arc<dyn Fn(PublishProgress) + Send + Sync>;

#[async_trait]
pub trait CloudPublisher: Send + Sync {
    async fn publish(
        &self,
        staged: StagedFile,
        intent: PublishIntent,
        progress: ProgressSink,
    ) -> CloudResult<CloudObjectRef>;
    async fn open_url(&self, object: &CloudObjectRef) -> CloudResult<url::Url>;
    /// 只在对象位置已入库后确认；失败可在下次启动重新对账。
    async fn confirm_committed(&self, intent: &PublishIntent) -> CloudResult<()>;
    async fn abandon(&self, account: &str, session: &str, file_id: u32) -> CloudResult<()>;
    /// 仅返回本机已确认远端完成的记录；账本是否提交仍由宿主判定。
    async fn completed_receipts(&self) -> CloudResult<Vec<PublishReceipt>>;
}

/// 上传已完成、等待宿主确认记账的本机恢复记录。
pub struct PublishReceipt {
    pub intent: PublishIntent,
    pub staged: StagedFile,
    pub object: CloudObjectRef,
}
