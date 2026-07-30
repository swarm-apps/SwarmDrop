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

    /// 邀请已过期。
    ///
    /// 变体名沿用「Code」是历史（6 位配对码时代），现在承载的是 PairInvite 的过期
    /// ——**不要照名字去理解语义**。改名要动 core / uniffi / TS bindings / 前端
    /// `KIND_MESSAGES` 四处的 `kind` 契约，与本次改动无关，故只在此说明。
    ///
    /// 面向用户的文案由前端按 `kind` 渲染，此处仅作语言无关技术描述。
    #[error("invite expired")]
    ExpiredCode,

    /// 邀请无效：解析 / 验签失败，或凭证已被消费、已撤销。
    ///
    /// 变体名同上，是历史遗留。技术细节（哪一步失败）只进日志 —— 它对用户没有意义，
    /// 而且是 Rust 侧的中文串，直接展示会在非中文界面露馅。
    #[error("invalid invite")]
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
