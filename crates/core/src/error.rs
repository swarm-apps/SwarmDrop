//! 平台无关错误类型。

use serde::Serialize;
use thiserror::Error;

/// Core 层统一错误类型。
#[derive(Debug, Error)]
pub enum AppError {
    /// 文件系统错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 序列化/反序列化错误
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// P2P 网络错误
    #[error("Network error: {0}")]
    Network(String),

    /// 身份/密钥对错误
    #[error("Identity error: {0}")]
    Identity(String),

    /// 节点未启动
    #[error("Node not started")]
    NodeNotStarted,

    /// 配对码已过期（面向用户的文案由前端按 `kind` 渲染，此处仅作语言无关技术描述）
    #[error("pairing code expired")]
    ExpiredCode,

    /// 无效的配对码（面向用户的文案由前端按 `kind` 渲染，此处仅作语言无关技术描述）
    #[error("invalid pairing code")]
    InvalidCode,

    /// tokio 任务错误
    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    /// 文件传输错误
    #[error("Transfer error: {0}")]
    Transfer(String),

    /// 数据库错误
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

/// 统一序列化为 `{ kind, message }`，便于各 host 投影到前端错误。
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("AppError", 2)?;

        let (kind, message) = match self {
            AppError::Io(e) => ("Io", e.to_string()),
            AppError::Serialization(e) => ("Serialization", e.to_string()),
            AppError::Network(msg) => ("Network", msg.clone()),
            AppError::Identity(msg) => ("Identity", msg.clone()),
            AppError::NodeNotStarted => ("NodeNotStarted", self.to_string()),
            AppError::ExpiredCode => ("ExpiredCode", self.to_string()),
            AppError::InvalidCode => ("InvalidCode", self.to_string()),
            AppError::TaskJoin(e) => ("TaskJoin", e.to_string()),
            AppError::Transfer(msg) => ("Transfer", msg.clone()),
            AppError::Database(e) => ("Database", e.to_string()),
        };

        state.serialize_field("kind", kind)?;
        state.serialize_field("message", &message)?;
        state.end()
    }
}

/// Result 类型别名。
pub type AppResult<T> = Result<T, AppError>;
