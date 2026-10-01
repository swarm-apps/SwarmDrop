//! 浏览器公开错误协议；native 同样编译以供类型导出。
use serde::Serialize;

/// Web 壳对外错误。`kind` 供 JS 分支，`message` 供展示。
///
/// wasm-bindgen 方法 reject 的错误值就是本类型的序列化对象（`{ kind, message }`）——
/// **不拍成字符串**（字符串丢了机器可判别的 kind）。JsValue 转换在 `error.rs`。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
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
    /// 调用被 `AbortSignal` 取消。**abort ≠ 撤回拨号**：Promise 立即 reject 且
    /// 无常驻意图残留，但在途拨号会继续到自然失败（libp2p 无逐次拨号取消面）。
    Aborted { message: String },
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

    pub fn aborted(message: impl Into<String>) -> Self {
        Self::Aborted {
            message: message.into(),
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            message: message.into(),
        }
    }
}

#[cfg(wasm_browser)]
mod browser;
#[cfg(wasm_browser)]
pub use browser::{WebResult, js_err, js_message};
