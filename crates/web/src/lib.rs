//! swarmdrop-web：浏览器 Web 壳。
//!
//! 让浏览器成为真正的 SwarmDrop 传输端——**包一层 core 的组合根** `start_node`（与桌面/移动
//! 同源装配），注入 Browser `EndpointProfile` + Web 端口（IndexedDB 写穿 store / OPFS /
//! ReadableStream 事件）。走完整 `NetManager` + 3 协议：配对经 `pair_with_invite`（真
//! capability 握手）。配对设备记录与传输会话都经 IndexedDB 持久化并在刷新后恢复——收件箱、
//! 传输历史与接收侧续传上下文跨刷新仍在（落库范围与浏览器侧的物理限制见 `store.rs`）。
//!
//! 除 [`types`]（JS 可见类型层，native 也编——specta 导出 test 在 native 注册它们）外，
//! 全部模块由 `cfg(wasm_browser)` 门控：native target 下近乎空 crate（`cargo check
//! --workspace` 秒过），只有 `wasm32-unknown-unknown` 下是真身。

pub mod types;

#[cfg(wasm_browser)]
mod abort;
#[cfg(wasm_browser)]
mod device_config;
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
mod idb;
#[cfg(wasm_browser)]
mod identity;
#[cfg(wasm_browser)]
mod inbox;
#[cfg(wasm_browser)]
mod invite_store;
#[cfg(wasm_browser)]
mod node;
#[cfg(wasm_browser)]
mod opfs;
#[cfg(wasm_browser)]
mod paired_devices;
#[cfg(wasm_browser)]
mod serialize;
#[cfg(wasm_browser)]
mod store;

#[cfg(wasm_browser)]
pub use node::WebNode;
pub use types::{
    ConnectionJson, Device, InboxHitFile, InboxItemDetail, InboxItemFileEntry, InboxItemSummary,
    InboxSearchHit, InviteListItemJson, OfferJson, PairInvitePreviewJson, PendingPairingJson,
    RelayInfoJson, RelayStateKind, WebError, WebTransferEvent,
};

/// wasm 模块加载即初始化 panic hook + tracing（浏览器 console）。
#[cfg(wasm_browser)]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
fn start() {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    console_error_panic_hook::set_once();

    // **按 target 分层过滤，不要全局 DEBUG。** libp2p 各层的 debug 日志量极大
    // （multistream 协商、每条连接的 poll、identify push…），全开会把本项目自己的
    // 日志冲出浏览器 console 的行数上限——排障时反而什么都看不到（实测吃过亏）。
    let filter = tracing_subscriber::filter::Targets::new()
        .with_target("swarmdrop_web", tracing::Level::DEBUG)
        .with_target("swarmdrop_core", tracing::Level::DEBUG)
        .with_target("swarmdrop_net", tracing::Level::DEBUG)
        .with_target("swarmdrop_transfer", tracing::Level::DEBUG)
        // 打洞信令的每一步都值得看见，它没有别的可观测手段
        .with_target("webrtc_p2p", tracing::Level::TRACE)
        .with_default(tracing::Level::INFO);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                // 浏览器无 std 时钟，不去掉会 runtime error。
                .without_time()
                .with_ansi(false)
                .with_writer(tracing_subscriber_wasm::MakeConsoleWriter::default()),
        )
        .with(filter)
        .init();
}
