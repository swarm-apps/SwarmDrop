use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CloudStorageError {
    #[error("云账户需要重新连接")]
    ReconnectRequired,
    #[error("云请求暂时失败，请重试")]
    Retryable,
    #[error("云盘权限或目标目录配置无效")]
    Configuration,
    #[error("云盘文件已被移除或无法访问")]
    ObjectUnavailable,
    #[error("云上传检查点保存或读取失败")]
    Checkpoint,
    #[error("暂存文件内容发生变化，请重新接收")]
    StagingChanged,
    #[error("云上传目标路径无效")]
    InvalidPath,
    #[error("云服务返回了无法识别的响应")]
    Protocol,
}
pub(crate) type StorageResult<T> = Result<T, CloudStorageError>;
pub type CloudResult<T> = Result<T, CloudFailure>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CloudOperation {
    Publish,
    Open,
    Confirm,
    Abandon,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CloudRecovery {
    Retry,
    Reconnect,
    CheckDestination,
    ReceiveAgain,
}

/// 只携带可安全进入 IPC 的上下文，不含 HTTP 响应或能力 URL。
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[error("{kind}")]
#[serde(rename_all = "camelCase")]
pub struct CloudFailure {
    pub kind: CloudStorageError,
    pub provider: swarmdrop_host::CloudProvider,
    pub account_id: Option<String>,
    pub operation: CloudOperation,
    pub recovery: CloudRecovery,
    pub retryable: bool,
    pub retry_after_seconds: Option<u64>,
}
impl CloudFailure {
    pub(crate) fn new(
        kind: CloudStorageError,
        provider: swarmdrop_host::CloudProvider,
        account: Option<&str>,
        operation: CloudOperation,
    ) -> Self {
        let recovery = match kind {
            CloudStorageError::ReconnectRequired => CloudRecovery::Reconnect,
            CloudStorageError::StagingChanged => CloudRecovery::ReceiveAgain,
            CloudStorageError::Retryable | CloudStorageError::Checkpoint => CloudRecovery::Retry,
            _ => CloudRecovery::CheckDestination,
        };
        Self {
            kind,
            provider,
            account_id: account.map(str::to_owned),
            operation,
            recovery,
            retryable: recovery == CloudRecovery::Retry,
            // 网络或限速重试耗尽后提供保守的退避建议，不把原始响应带过边界。
            retry_after_seconds: (kind == CloudStorageError::Retryable).then_some(30),
        }
    }
}
