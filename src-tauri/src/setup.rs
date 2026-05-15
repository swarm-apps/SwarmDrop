//! Tauri Builder 装配
//!
//! 把 [`crate::run`] 拆成四段：tracing 初始化 / plugin 注册 / setup hook /
//! 命令注册。每段各司其职，方便调整。

use tauri::{Builder, Manager, Wry};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::commands;

/// 初始化 tracing 订阅器（默认 `swarmdrop=debug,swarm_p2p_core=debug`，可被
/// `RUST_LOG` 覆盖）
pub fn init_tracing() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("swarmdrop=debug,swarm_p2p_core=debug")),
        )
        .init();
}

/// 构造完整的 [`Builder`]：plugin → setup hook → invoke handler。
pub fn build_app() -> Builder<Wry> {
    let builder = register_plugins(Builder::default());
    register_setup(register_handlers(builder))
}

/// 注册所有官方 + 第三方 plugin。
fn register_plugins(builder: Builder<Wry>) -> Builder<Wry> {
    builder
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_biometry::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
}

/// 注册所有 IPC 命令。
fn register_handlers(builder: Builder<Wry>) -> Builder<Wry> {
    builder.invoke_handler(tauri::generate_handler![
        // lifecycle
        commands::start,
        commands::shutdown,
        commands::list_devices,
        commands::get_network_status,
        commands::install_update,
        // identity
        commands::initialize_identity,
        commands::generate_keypair,
        commands::register_keypair,
        // pairing
        commands::generate_pairing_code,
        commands::get_device_info,
        commands::request_pairing,
        commands::respond_pairing_request,
        commands::remove_paired_device,
        // transfer
        commands::scan_sources,
        commands::prepare_send,
        commands::start_send,
        commands::accept_receive,
        commands::reject_receive,
        commands::cancel_send,
        commands::cancel_receive,
        commands::get_transfer_history,
        commands::get_transfer_session,
        commands::delete_transfer_session,
        commands::clear_transfer_history,
        commands::pause_transfer,
        commands::resume_transfer,
        // mcp
        commands::get_mcp_status,
        commands::start_mcp_server,
        commands::stop_mcp_server,
    ])
}

/// `setup` hook：updater plugin 容错注册 + 数据库初始化 + MCP state 装配
fn register_setup(builder: Builder<Wry>) -> Builder<Wry> {
    builder.setup(|app| {
        // updater plugin —— 移动端不支持时静默跳过
        if let Err(e) = app
            .handle()
            .plugin(tauri_plugin_updater::Builder::new().build())
        {
            tracing::warn!("Failed to initialize updater plugin: {e}");
        }

        // 数据库（SeaORM + SQLite）—— 同步执行 + 启动清理过期会话
        let handle = app.handle().clone();
        let db = tauri::async_runtime::block_on(crate::database::init_database(&handle))?;
        tauri::async_runtime::block_on(crate::database::cleanup_stale_sessions(&db))?;
        app.manage(db);

        // MCP server 状态容器
        app.manage(crate::mcp::server::McpServerState::default());

        Ok(())
    })
}
