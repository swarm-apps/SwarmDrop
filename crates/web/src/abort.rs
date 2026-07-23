//! [`web_sys::AbortSignal`] → future 的单点封装。
//!
//! JS 侧超时/取消组合全部用平台原语表达（`AbortSignal.timeout()` /
//! `AbortSignal.any()`），本 crate 不自造 `timeoutMs` 类参数。监听器经
//! RAII guard 在 future drop 时摘除（select 另一分支先完成的路径也不泄漏）。

use std::future::Future;

use futures::channel::oneshot;
use futures::future::{Either, select};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

/// "abort" 事件监听的 RAII guard：drop 时摘监听（Closure 一并回收）。
struct ListenerGuard {
    signal: web_sys::AbortSignal,
    closure: Closure<dyn FnMut()>,
}

impl ListenerGuard {
    fn attach(signal: web_sys::AbortSignal, closure: Closure<dyn FnMut()>) -> Self {
        let _ = signal.add_event_listener_with_callback("abort", closure.as_ref().unchecked_ref());
        Self { signal, closure }
    }
}

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        let _ = self
            .signal
            .remove_event_listener_with_callback("abort", self.closure.as_ref().unchecked_ref());
    }
}

/// 业务 future 与可选 abort 信号赛跑（select 编排的单点收口）。
///
/// 返回 `None` = 被 abort（取消错误的文案由调用方构造）；无 signal 时
/// 直接等业务完成。所有接 `AbortSignal` 的 wasm API 都经此组合，不再
/// 各自手写 pin_mut + select 样板。
pub async fn race<F: Future>(signal: Option<web_sys::AbortSignal>, fut: F) -> Option<F::Output> {
    match signal {
        Some(sig) => {
            let abort = wait_abort(sig);
            futures::pin_mut!(fut, abort);
            match select(fut, abort).await {
                Either::Left((out, _)) => Some(out),
                Either::Right(((), _)) => None,
            }
        }
        None => Some(fut.await),
    }
}

/// 等待 signal 触发 abort（已 aborted 时立即返回）。
///
/// 与业务 future 一起 `select`：本分支先完成即「调用被取消」。
pub async fn wait_abort(signal: web_sys::AbortSignal) {
    if signal.aborted() {
        return;
    }
    let (tx, rx) = oneshot::channel::<()>();
    let mut tx = Some(tx);
    let closure = Closure::wrap(Box::new(move || {
        if let Some(tx) = tx.take() {
            let _ = tx.send(());
        }
    }) as Box<dyn FnMut()>);
    let _guard = ListenerGuard::attach(signal, closure);
    // sender 已在 guard 存活期内：rx 出错（不可能路径）也按 aborted 处理
    let _ = rx.await;
}
