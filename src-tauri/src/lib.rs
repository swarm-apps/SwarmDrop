//! SwarmDrop Tauri 桌面壳
//!
//! 业务逻辑全部在 [`swarmdrop_core`]，本 crate 只承担：
//! 1. Tauri Builder 构造（plugins / setup / handler）—— 见 [`setup`]
//! 2. host adapter 实现 —— 见 [`host`]
//! 3. Tauri IPC 命令薄壳 —— 见 [`commands`]

pub mod commands;
pub(crate) mod database;
pub mod device;
pub mod error;
pub mod events;
pub mod host;
pub(crate) mod mcp;
pub(crate) mod network;
pub(crate) mod setup;

pub use error::{AppError, AppResult};

/// 应用入口（main.rs 调用）。
#[doc(alias = "main")]
pub fn run() {
    setup::init_tracing();
    setup::build_app()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
