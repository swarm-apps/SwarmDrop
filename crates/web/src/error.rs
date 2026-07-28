//! Web 壳错误的转换层。
//!
//! 类型定义（[`WebError`]，`{ kind, message }`）在 [`crate::types`]（native 也编，specta
//! 导出 TS 形状）；本模块只放 wasm 侧转换：序列化成 JsValue、从内核错误收敛。

use swarmdrop_host::AppError;
use wasm_bindgen::{JsCast, JsValue};

pub use crate::types::WebError;

impl WebError {
    /// 序列化成结构化 JS 对象；序列化本身失败时兜底成字符串（不应发生）。
    pub fn to_js(&self) -> JsValue {
        crate::serialize::to_js(self)
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

/// Web 侧错误 → 内核错误（`error.rs` 里 `From<AppError> for WebError` 的反向）。
///
/// 用在 Web 端口实现（`store.rs` 的 IndexedDB 写）里：端口签名返回 `AppResult`，
/// 而底层失败是 `WebError`。收敛到 `Transfer` 一类——对内核而言这就是「持久化没成功」，
/// 更细的 kind 在 message 里保留。
impl From<WebError> for AppError {
    fn from(e: WebError) -> Self {
        let message = match &e {
            WebError::Identity { message }
            | WebError::Network { message }
            | WebError::Transfer { message }
            | WebError::InvalidInput { message }
            | WebError::Aborted { message }
            | WebError::NotFound { message }
            | WebError::Storage { message } => message,
        };
        AppError::Transfer(message.clone())
    }
}

/// `impl Display` 的内核错误 → JsValue 的便捷转换（用于 net 层的 Display 错误）。
pub fn js_err(e: impl std::fmt::Display) -> JsValue {
    WebError::network(e.to_string()).to_js()
}

/// JS 抛出的值 → 人话消息。
///
/// `DomException` / `{ message }` / 裸字符串三种形态都取到那句话。**不要用 `{:?}`**——
/// `JsValue` 的 Debug 会打成 `JsValue(DOMException(...))` 之类的噪音，而这些消息（OPFS 落盘
/// 失败、IndexedDB 配额拒绝）是直接渲染给用户看的。取不到就退回一句兜底描述。
pub fn js_message(value: &JsValue, fallback: &str) -> String {
    value
        .dyn_ref::<web_sys::DomException>()
        .map(|e| e.message())
        .or_else(|| {
            js_sys::Reflect::get(value, &JsValue::from_str("message"))
                .ok()
                .and_then(|v| v.as_string())
        })
        .or_else(|| value.as_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// `Result<T, WebError>` → `Result<T, JsValue>` 的收尾。
pub type WebResult<T> = Result<T, WebError>;

impl From<WebError> for JsValue {
    fn from(e: WebError) -> Self {
        e.to_js()
    }
}
