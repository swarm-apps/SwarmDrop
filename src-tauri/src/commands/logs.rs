//! 日志目录命令薄壳
//!
//! 日志的产生与落盘在 [`crate::logging`]，这里只负责把目录交给系统文件管理器。
//!
//! 给「打开文件夹」而不是「导出日志」：桌面端本来就有文件管理器，而且用户能先自己
//! 看一眼再决定发不发——日志里有设备标识与网络地址，这个默认更保守。
//! （移动端做的是导出 + 分享，因为那里没有文件管理器这个概念。）

use tauri::{AppHandle, Manager};

/// 用系统文件管理器打开应用日志目录。
///
/// 目录不存在时返回错误而非静默成功——那通常意味着文件层没装上（见
/// [`crate::logging::install_file_layer`]），让用户看到比假装打开了更有用。
#[tauri::command]
#[specta::specta]
pub fn open_log_dir(app: AppHandle) -> crate::AppResult<()> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| crate::AppError::transfer(format!("取日志目录失败: {e}")))?;

    if !dir.exists() {
        return Err(crate::AppError::transfer(
            "日志目录尚未创建——可能是文件日志未能初始化".to_string(),
        ));
    }

    // 与 `commands/inbox.rs` 用同一个 opener API。
    tauri_plugin_opener::open_path(&dir, None::<&str>)
        .map_err(|e| crate::AppError::transfer(format!("打开日志目录失败: {e}")))?;

    Ok(())
}
