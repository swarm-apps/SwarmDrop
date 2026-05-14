pub mod commands;
pub(crate) mod database;
pub mod error;
pub mod events;
pub(crate) mod mcp;
pub(crate) mod network;
pub(crate) mod transfer;
pub use error::{AppError, AppResult};

pub mod file_sink;
pub mod file_source;
pub mod host;

// 命名空间 alias —— 直接复用 swarmdrop-core 的业务模块，
// 让 `crate::pairing::*` / `crate::protocol::*` / `crate::device` 在原引用点无需修改。
pub use swarmdrop_core::pairing;
pub use swarmdrop_core::protocol;
pub mod device {
    pub use swarmdrop_core::device::*;
    pub use swarmdrop_core::device_manager::*;
}

use tauri::Manager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn init_tracing() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("swarmdrop=debug,swarm_p2p_core=debug")),
        )
        .init();
}

pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_biometry::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // updater 在 setup 中注册，移动端不支持时容错跳过
            if let Err(e) = app
                .handle()
                .plugin(tauri_plugin_updater::Builder::new().build())
            {
                tracing::warn!("Failed to initialize updater plugin: {e}");
            }

            // 初始化数据库（SeaORM + SQLite）
            let handle = app.handle().clone();
            let db = tauri::async_runtime::block_on(database::init_database(&handle))?;

            // 启动清理：处理上次运行中断的传输会话
            tauri::async_runtime::block_on(database::cleanup_stale_sessions(&db))?;

            app.manage(db);

            // 初始化 MCP Server 状态容器
            app.manage(mcp::server::McpServerState::default());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start,
            commands::shutdown,
            commands::initialize_identity,
            commands::generate_keypair,
            commands::register_keypair,
            commands::generate_pairing_code,
            commands::get_device_info,
            commands::request_pairing,
            commands::respond_pairing_request,
            commands::remove_paired_device,
            commands::list_devices,
            commands::get_network_status,
            commands::install_update,
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
            commands::get_mcp_status,
            commands::start_mcp_server,
            commands::stop_mcp_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
