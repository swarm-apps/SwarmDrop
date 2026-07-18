//! Web 壳的可序列化错误。
//!
//! wasm-bindgen 方法返回 `Result<T, JsValue>`；错误经 serde-wasm-bindgen 序列化成结构化
//! JS 对象（`{ kind, message }`）——**不拍成字符串**（knowledge 记录过该坑，字符串丢了
//! 机器可判别的 kind）。

use serde::Serialize;
use swarmdrop_host::AppError;
use wasm_bindgen::JsValue;

/// Web 壳对外错误。`kind` 供 JS 分支，`message` 供展示。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WebError {
    /// 身份 / 密钥错误。
    Identity { message: String },
    /// 网络 / 连接 / DHT 错误。
    Network { message: String },
    /// 传输错误。
    Transfer { message: String },
    /// 入参非法（地址格式、缺 `/p2p/` 等）。
    InvalidInput { message: String },
    /// 分享码不存在 / 已过期。
    NotFound { message: String },
    /// 存储（OPFS / localStorage）错误。
    Storage { message: String },
}

impl WebError {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            message: message.into(),
        }
    }

    /// 序列化成结构化 JS 对象；序列化本身失败时兜底成字符串（不应发生）。
    pub fn to_js(&self) -> JsValue {
        serde_wasm_bindgen::to_value(self)
            .unwrap_or_else(|_| JsValue::from_str("error serialization failed"))
    }
}

impl From<AppError> for WebError {
    fn from(e: AppError) -> Self {
        // AppError 已有 kind 语义；Web 侧收敛到 Network/Transfer/Identity 三类 + 兜底。
        match &e {
            AppError::Network(_) | AppError::NodeNotStarted => Self::Network {
                message: e.to_string(),
            },
            AppError::Identity(_) | AppError::ExpiredCode | AppError::InvalidCode => {
                Self::Identity {
                    message: e.to_string(),
                }
            }
            _ => Self::Transfer {
                message: e.to_string(),
            },
        }
    }
}

/// `impl Display` 的内核错误 → JsValue 的便捷转换（用于 net 层的 Display 错误）。
pub fn js_err(e: impl std::fmt::Display) -> JsValue {
    WebError::network(e.to_string()).to_js()
}

/// `impl Display` 的内核错误 → [`WebError::Network`]。
pub fn js_err_to_web(e: impl std::fmt::Display) -> WebError {
    WebError::network(e.to_string())
}

/// `Result<T, WebError>` → `Result<T, JsValue>` 的收尾。
pub type WebResult<T> = Result<T, WebError>;

impl From<WebError> for JsValue {
    fn from(e: WebError) -> Self {
        e.to_js()
    }
}
