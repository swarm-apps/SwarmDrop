//! Tauri IPC 命令薄壳
//!
//! 每个命令都是 `#[tauri::command]`，**不持有业务逻辑**：
//! - 解析参数 / 取 Tauri State
//! - 调用 [`swarmdrop_core`] 中对应的 manager
//! - 包装返回值
//!
//! 命令按业务域分文件：
//! - [`lifecycle`] —— 应用 / 网络生命周期、设备列表、应用更新
//! - [`identity`] —— 身份密钥
//! - [`pairing`] —— 设备配对
//! - [`transfer`] —— 文件传输
//! - [`mcp`] —— MCP server 控制

/// 从 NetManagerState 获取 manager 引用并执行表达式（短暂持锁）
///
/// pairing.rs / 其他命令公用，避免每处都写一遍 `let guard = net.lock().await; ...`。
#[macro_export]
#[doc(hidden)]
macro_rules! with_manager {
    ($net:expr, |$m:ident| $body:expr) => {{
        let guard = $net.lock().await;
        let $m = guard
            .as_ref()
            .ok_or_else(|| $crate::AppError::node_not_started())?;
        Ok::<_, $crate::AppError>($body?)
    }};
}

mod external_open;
mod i18n;
mod identity;
mod inbox;
mod lifecycle;
mod mcp;
mod pairing;
mod transfer;

// glob re-export：Tauri 的 #[tauri::command] 宏会生成 __cmd__* 隐藏符号，
// generate_handler! 需要通过模块路径访问这些符号，显式导出无法覆盖。
pub use external_open::*;
pub use i18n::*;
pub use identity::*;
pub use inbox::*;
pub use lifecycle::*;
pub use mcp::*;
pub use pairing::*;
pub use transfer::*;
