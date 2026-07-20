//! Window / Worker 双环境自适应。
//!
//! wasm 既跑主线程（Window，webrtc+ws 双 transport）也跑 Web Worker（ws-only 传输 +
//! OPFS 落盘的 Worker 版）：用 `js_sys::global()` 探测环境取 navigator / isSecureContext。
//! localStorage 仅 Window 有——Worker 环境的身份持久化退到 OPFS（见 identity.rs）。
//!
//! 注意：webrtc-websys 的 `window()` panic 只在 **dial webrtc-direct 地址**时触发
//! （`Transport::dial → maybe_local_firefox`），构造无害——Worker 里装着 webrtc transport
//! 不拨它即可，无需拆 preset。

use wasm_bindgen::JsCast;
use web_sys::{Storage, StorageManager, WorkerGlobalScope};

/// Worker 全局作用域（非 Worker 环境返回 None）。
fn worker_scope() -> Option<WorkerGlobalScope> {
    js_sys::global().dyn_into::<WorkerGlobalScope>().ok()
}

/// 当前全局环境是否 secure context（Window 与 Worker 都有该属性）。
pub fn is_secure_context() -> bool {
    if let Some(win) = web_sys::window() {
        return win.is_secure_context();
    }
    worker_scope()
        .map(|w| w.is_secure_context())
        .unwrap_or(false)
}

/// `navigator.storage`（OPFS 入口），Window / Worker 通吃。
pub fn storage_manager() -> Option<StorageManager> {
    if let Some(win) = web_sys::window() {
        return Some(win.navigator().storage());
    }
    worker_scope().map(|w| w.navigator().storage())
}

/// localStorage：仅 Window 环境有，Worker 返回 None。
pub fn local_storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// `navigator.userAgent`，Window / Worker 通吃（取不到返回空串）。
pub fn user_agent() -> String {
    if let Some(win) = web_sys::window() {
        return win.navigator().user_agent().unwrap_or_default();
    }
    worker_scope()
        .and_then(|w| w.navigator().user_agent().ok())
        .unwrap_or_default()
}
