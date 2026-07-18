//! swarmdrop-web：浏览器 Web 壳。
//!
//! 让浏览器成为真正的 SwarmDrop 传输端——offer/accept/续传/bao 逐块验证**全量复用**
//! `swarmdrop-transfer`，端口（[`SessionStore`]/[`FileAccess`]/[`TransferEventSink`]）用
//! Web 实现（内存表 / OPFS / ReadableStream）填充。范围内**无配对持久化**（正式配对 /
//! React UI 属后续前端工程）。
//!
//! 整 crate 由 `#![cfg(wasm_browser)]` 门控：native target 下是空 crate（`cargo check
//! --workspace` 秒过），只有 `wasm32-unknown-unknown` 下是真身。
#![cfg(wasm_browser)]

mod error;
mod events;
mod file_access;
mod identity;
mod node;
mod share_code;
mod store;

pub use node::WebNode;

use wasm_bindgen::prelude::*;

/// wasm 模块加载即初始化 panic hook + tracing（浏览器 console）。
#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
    tracing_subscriber::fmt()
        // 浏览器无 std 时钟，不去掉会 runtime error。
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(tracing_subscriber_wasm::MakeConsoleWriter::default())
        .init();
}
