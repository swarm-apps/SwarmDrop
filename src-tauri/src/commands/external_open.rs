//! 外部「用 SwarmDrop 打开」命令薄壳
//!
//! 业务/OS 集成逻辑全部在 [`crate::external_open`]，这里只做委托。

use crate::external_open;

/// 前端根处理器 mount 时调用：标记就绪并取走冷启动期间缓冲的外部打开路径。
#[tauri::command]
#[specta::specta]
pub async fn take_pending_external_open() -> crate::AppResult<Vec<String>> {
    Ok(external_open::take_pending())
}
