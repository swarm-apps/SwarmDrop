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
pub mod external_open;
pub mod host;
pub(crate) mod mcp;
pub(crate) mod network;
pub mod setup;
pub mod tray;

pub use error::{AppError, AppResult};
pub use setup::specta_builder;

/// 应用入口（main.rs 调用）。
#[doc(alias = "main")]
pub fn run() {
    setup::init_tracing();
    setup::build_app()
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // macOS「Open With / 拖到 Dock 图标」经 RunEvent::Opened 送达文件 URL；
            // 归一化 + 唤窗 + 分发都在 external_open 内部。Windows / Linux 走 argv +
            // single-instance（见 setup.rs），不经此分支。
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = event {
                // Opened 在 ObjC extern "C" 回调里触发，panic 不能跨该边界 unwind（否则
                // 直接 abort）。catch_unwind 兜底，把任何 panic 降级为日志而非崩溃。
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    external_open::handle_opened(app_handle, &urls);
                }));
                if result.is_err() {
                    tracing::error!("external open: handle_opened panicked (ignored)");
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app_handle, event);
        });
}
