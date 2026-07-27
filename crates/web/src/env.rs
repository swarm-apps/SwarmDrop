//! Window / Worker 双环境自适应。
//!
//! wasm 既跑主线程（Window，webrtc+ws 双 transport）也跑 Web Worker（ws-only 传输 +
//! OPFS 落盘的 Worker 版）：用 `js_sys::global()` 探测环境取 navigator / isSecureContext。
//! localStorage 仅 Window 有——Worker 环境的身份持久化退到 OPFS（见 identity.rs）。
//!
//! 注意：webrtc-websys 在 Worker 里**不能装**，不是「装了不拨就行」。它的 `dial` 在地址
//! 格式检查**之前**就调 `maybe_local_firefox()`（内含 `window().expect`），于是经
//! `or_transport` 拨任何地址（含 ws）都先进 webrtc 分支碰 panic。2026-07-18 Worker 版
//! 基准实测坐实，故 `transport.rs` 在无 window 时直接返回 ws-only 栈。

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
