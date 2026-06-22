//! 桌面端错误类型 —— 仅承载 Tauri-specific 错误，业务错误全部委托给 [`swarmdrop_core::AppError`]。
//!
//! ## 设计
//!
//! - `Tauri(...)` 包 [`tauri::Error`]（plugin/Manager/IPC 等只在桌面壳出现的错误）
//! - `Core(...)` 包 [`swarmdrop_core::AppError`]（一切业务错误：Io / Serialization / P2p /
//!   Database / Transfer / NodeNotStarted / ExpiredCode / InvalidCode / TaskJoin / ...）
//!
//! 调用方只需 `?` 即可：std::io::Error → core::AppError::Io → AppError::Core；
//! tauri::Error → AppError::Tauri。host adapter 直接产 [`swarmdrop_core::AppResult`]，
//! 不再需要 to_core_error 手工转换。
//!
//! 由 `Serialize` 投影到前端的格式与 core 一致：`{ kind, message }`。
//! `kind` 优先取 core 自带的 kind（让前端 `isErrorKind` 与单仓时表现完全一致）。

use serde::Serialize;
use thiserror::Error;

/// 前端可见的错误结构体 —— [`AppError`] 在 specta 里被映射成这个形状，
/// 与 [`AppError::serialize`] 真正写出的 JSON 完全一致。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorPayload {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum AppError {
    /// Tauri-specific 错误：plugin、Manager、IPC、updater 等
    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),

    /// 平台无关业务错误（Io / Database / Transfer / NodeNotStarted / ...）
    #[error(transparent)]
    Core(#[from] swarmdrop_core::AppError),
}

// 手写 Type impl：AppError 的 Serialize 输出就是 AppErrorPayload，
// 直接复用 payload 的定义，避免重复维护两份 schema。
impl specta::Type for AppError {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <AppErrorPayload as specta::Type>::definition(types)
    }
}

// ============ 让常见错误源直接 `?` 转 AppError，无需先经 core ============

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Core(e.into())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Core(e.into())
    }
}

impl From<swarm_p2p_core::Error> for AppError {
    fn from(e: swarm_p2p_core::Error) -> Self {
        AppError::Core(e.into())
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(e: sea_orm::DbErr) -> Self {
        AppError::Core(e.into())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        AppError::Core(e.into())
    }
}

// ============ 命令薄壳里仍想直接构造业务错误的便捷构造器 ============

impl AppError {
    pub fn transfer<S: Into<String>>(msg: S) -> Self {
        AppError::Core(swarmdrop_core::AppError::Transfer(msg.into()))
    }
    pub fn identity<S: Into<String>>(msg: S) -> Self {
        AppError::Core(swarmdrop_core::AppError::Identity(msg.into()))
    }
    pub fn network<S: Into<String>>(msg: S) -> Self {
        AppError::Core(swarmdrop_core::AppError::Network(msg.into()))
    }
    pub fn node_not_started() -> Self {
        AppError::Core(swarmdrop_core::AppError::NodeNotStarted)
    }
}

// ============ 前端友好的 { kind, message } 序列化 ============

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            // core 错误：直接委托 core 的 Serialize impl，kind 取 core 的 kind
            AppError::Core(e) => e.serialize(serializer),
            // Tauri 错误：包成 { kind: "Tauri", message }
            AppError::Tauri(e) => {
                use serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct("AppError", 2)?;
                state.serialize_field("kind", "Tauri")?;
                state.serialize_field("message", &e.to_string())?;
                state.end()
            }
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
