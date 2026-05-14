//! FFI 错误类型 —— `AppError` 的镜像,跨 mobile/桌面 共享 kind 名作为合约。
//!
//! - 不要给共享 crate `swarmdrop-core` 的 `AppError` 加 `uniffi::Error`,会污染桌面端
//! - kind 名字一一对应 `AppError` 变体,RN 侧用 `FfiError.<Variant>.instanceOf(e)` 判别

use swarmdrop_core::AppError;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("io error: {0}")]
    Io(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("identity error: {0}")]
    Identity(String),
    #[error("node not started")]
    NodeNotStarted,
    #[error("pairing code expired")]
    ExpiredCode,
    #[error("invalid pairing code")]
    InvalidCode,
    #[error("transfer error: {0}")]
    Transfer(String),
    #[error("database error: {0}")]
    Database(String),
}

impl From<AppError> for FfiError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Io(error) => Self::Io(error.to_string()),
            AppError::Serialization(error) => Self::Serialization(error.to_string()),
            AppError::P2p(error) => Self::Network(error.to_string()),
            AppError::Network(message) => Self::Network(message),
            AppError::Identity(message) => Self::Identity(message),
            AppError::NodeNotStarted => Self::NodeNotStarted,
            AppError::ExpiredCode => Self::ExpiredCode,
            AppError::InvalidCode => Self::InvalidCode,
            AppError::TaskJoin(error) => Self::Network(error.to_string()),
            AppError::Transfer(message) => Self::Transfer(message),
            AppError::Database(error) => Self::Database(error.to_string()),
        }
    }
}

impl From<FfiError> for AppError {
    fn from(error: FfiError) -> Self {
        match error {
            FfiError::Io(message) => AppError::Io(std::io::Error::other(message)),
            FfiError::Serialization(message) => AppError::Identity(message),
            FfiError::Network(message) => AppError::Network(message),
            FfiError::Identity(message) => AppError::Identity(message),
            FfiError::NodeNotStarted => AppError::NodeNotStarted,
            FfiError::ExpiredCode => AppError::ExpiredCode,
            FfiError::InvalidCode => AppError::InvalidCode,
            FfiError::Transfer(message) => AppError::Transfer(message),
            FfiError::Database(message) => AppError::Identity(message),
        }
    }
}

pub type FfiResult<T> = Result<T, FfiError>;
