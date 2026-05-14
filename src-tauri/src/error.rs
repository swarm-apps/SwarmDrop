//! 应用错误处理模块
//!
//! Tauri 命令的错误必须实现 Serialize 才能传递给前端

use serde::Serialize;
use thiserror::Error;

/// 应用统一错误类型
///
/// 注意：使用 `#[from]` 的变体会存储原始错误类型，
/// 但由于 `std::io::Error` 等不实现 `Serialize`，
/// 通过自定义 Serialize 实现统一转为 `{ kind, message }` 格式。
#[derive(Debug, Error)]
pub enum AppError {
    /// 文件系统错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 序列化/反序列化错误
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Tauri 错误
    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),

    /// 平台无关 core 错误
    #[error("Core error: {0}")]
    Core(#[from] swarmdrop_core::AppError),

    /// P2P 核心库错误
    #[error("P2P error: {0}")]
    P2p(#[from] swarm_p2p_core::Error),

    /// P2P 网络错误
    #[error("Network error: {0}")]
    Network(String),

    /// 身份/密钥对错误
    #[error("Identity error: {0}")]
    Identity(String),

    /// 节点未启动
    #[error("Node not started")]
    NodeNotStarted,

    /// 配对码已过期
    #[error("配对码已过期")]
    ExpiredCode,

    /// 无效的配对码
    #[error("无效的配对码")]
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

/// 传递给前端的序列化错误格式
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
            AppError::Tauri(e) => ("Tauri", e.to_string()),
            AppError::Core(e) => ("Core", e.to_string()),
            AppError::P2p(e) => ("P2p", e.to_string()),
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

// ============ 便捷类型别名 ============

/// Result 类型别名
pub type AppResult<T> = Result<T, AppError>;
