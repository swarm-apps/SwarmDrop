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

/// 当前是否 Window（主线程）环境；false = Worker。
pub fn is_window() -> bool {
    web_sys::window().is_some()
}

/// 当前全局环境是否 secure context（Window 与 Worker 都有该属性）。
pub fn is_secure_context() -> bool {
    if let Some(win) = web_sys::window() {
        return win.is_secure_context();
    }
    js_sys::global()
        .dyn_into::<WorkerGlobalScope>()
        .map(|w| w.is_secure_context())
        .unwrap_or(false)
}

/// `navigator.storage`（OPFS 入口），Window / Worker 通吃。
pub fn storage_manager() -> Option<StorageManager> {
    if let Some(win) = web_sys::window() {
        return Some(win.navigator().storage());
    }
    js_sys::global()
        .dyn_into::<WorkerGlobalScope>()
        .ok()
        .map(|w| w.navigator().storage())
}

/// localStorage：仅 Window 环境有，Worker 返回 None。
pub fn local_storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
