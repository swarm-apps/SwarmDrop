//! swarmdrop-web：浏览器 Web 壳。
//!
//! 让浏览器成为真正的 SwarmDrop 传输端——**包一层 core 的组合根** `start_node`（与桌面/移动
//! 同源装配），注入 Browser `EndpointProfile` + Web 端口（内存 store / OPFS / ReadableStream
//! 事件）。走完整 `NetManager` + 3 协议：配对经 `pair_with_invite`（真 capability 握手），
//! 配对记录**内存态**（IndexedDB 持久化 + React UI 属后续前端工程）。
//!
//! 除 [`types`]（JS 可见类型层，native 也编——specta 导出 test 在 native 注册它们）外，
//! 全部模块由 `cfg(wasm_browser)` 门控：native target 下近乎空 crate（`cargo check
//! --workspace` 秒过），只有 `wasm32-unknown-unknown` 下是真身。

pub mod types;

#[cfg(wasm_browser)]
mod abort;
#[cfg(wasm_browser)]
mod env;
#[cfg(wasm_browser)]
mod error;
#[cfg(wasm_browser)]
mod event_bus;
#[cfg(wasm_browser)]
mod events;
#[cfg(wasm_browser)]
mod file_access;
#[cfg(wasm_browser)]
mod identity;
#[cfg(wasm_browser)]
mod node;
#[cfg(wasm_browser)]
mod opfs;
#[cfg(wasm_browser)]
mod store;

#[cfg(wasm_browser)]
pub use node::WebNode;
pub use types::{
    ConnectionJson, OfferJson, PendingPairingJson, RelayInfoJson, RelayStateKind, WebError,
    WebTransferEvent,
};

/// wasm 模块加载即初始化 panic hook + tracing（浏览器 console）。
#[cfg(wasm_browser)]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
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
