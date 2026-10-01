use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CloudAuthError {
    #[error("云账户配置无效")]
    InvalidConfiguration,
    #[error("云账户需要重新连接")]
    ReconnectRequired,
    #[error("云凭证保存失败")]
    SaveFailed,
    #[error("云凭证读取失败")]
    ReadFailed,
    #[error("云凭证删除失败")]
    DeleteFailed,
    #[error("云服务暂时不可达，请稍后重试")]
    Unavailable,
    #[error("授权被拒绝")]
    Denied,
    #[error("授权等待超时")]
    Timeout,
    #[error("云账户不存在")]
    AccountNotFound,
    #[error("授权会话已取消")]
    Cancelled,
}
