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
        // **穷尽 match，不留 catch-all。** 这里曾是 `_ => Transfer`，于是内核每加一个
        // kind，Web 就默默把它显示成「文件传输失败，请重试」——`kind` 是前端渲染文案的
        // 判别码，落错分类就是给用户一句与真实原因无关的提示。写成穷尽之后，
        // 新增变体会在这里编译失败，逼调用方想一下「浏览器该怎么说这件事」。
        let message = e.to_string();
        match &e {
            AppError::Network(_) | AppError::NodeNotStarted => Self::Network { message },
            // 身份类：密钥材料读写失败 / 还没就绪；邀请凭证本身的问题（过期、无效）
            // 沿用既有归类。
            AppError::Identity(_)
            | AppError::IdentityNotReady
            | AppError::ExpiredCode
            | AppError::InvalidCode => Self::Identity { message },
            // 入参非法：地址格式、标识格式等，用户无能为力。
            AppError::InvalidArgument(_) => Self::InvalidInput { message },
            AppError::DeviceNotFound => Self::NotFound { message },
            // 邀请状态没写进 IndexedDB —— 本质就是一次存储失败。
            AppError::InvitePersistFailed | AppError::Database(_) | AppError::Io(_) => {
                Self::Storage { message }
            }
            AppError::Serialization(_) | AppError::TaskJoin(_) | AppError::Transfer(_) => {
                Self::Transfer { message }
            }
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
