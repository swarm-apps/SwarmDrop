//! 语言设置命令薄壳
//!
//! 业务逻辑在 [`crate::i18n`]，这里只做委托。前端 `preferences-store.setLocale` 在
//! `dynamicActivate` 之后调用，把 locale 推给桌面壳的 rust-i18n（托盘 + 通知随之切换）。

use tauri::AppHandle;

/// 应用当前 locale：更新 rust-i18n 全局 locale 并即时重绘托盘菜单文案。
#[tauri::command]
#[specta::specta]
pub fn set_locale(app: AppHandle, locale: String) -> crate::AppResult<()> {
    crate::i18n::set_locale(&locale);
    crate::tray::relocalize_tray(&app);
    Ok(())
}
