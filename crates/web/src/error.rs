//! Web 壳错误的转换层。
//!
//! 类型定义（[`WebError`]，`{ kind, message }`）在 [`crate::types`]（native 也编，specta
//! 导出 TS 形状）；本模块只放 wasm 侧转换：序列化成 JsValue、从内核错误收敛。

use swarmdrop_host::AppError;
use wasm_bindgen::JsValue;

pub use crate::types::WebError;

impl WebError {
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

/// `Result<T, WebError>` → `Result<T, JsValue>` 的收尾。
pub type WebResult<T> = Result<T, WebError>;

impl From<WebError> for JsValue {
    fn from(e: WebError) -> Self {
        e.to_js()
    }
}
