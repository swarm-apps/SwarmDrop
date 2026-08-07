//! 外部入口命令薄壳（「用 SwarmDrop 打开」+ `swarmdrop://` 深链）
//!
//! 业务/OS 集成逻辑全部在 [`crate::external_open`]，这里只做委托。

use crate::external_open::{self, PendingExternalOpen};

/// 前端根处理器 mount 时调用：标记就绪并**一次取走**冷启动期间缓冲的两类负载
/// （文件路径 + 深链邀请）。
///
/// 一次取走而非两个命令：`frontend_ready` 是共享标记，拆开会让第二类负载丢在
/// 「标记已置位、前端还没订阅完」那道缝里（详见 [`external_open::take_pending`]）。
#[tauri::command]
#[specta::specta]
pub async fn take_pending_external_open() -> crate::AppResult<PendingExternalOpen> {
    Ok(external_open::take_pending())
}
