//! 桌面壳原生字符串本地化（托盘 + 系统通知）。
//!
//! 前端 `preferences-store` 是 locale 权威源；本模块只负责把当前 locale 喂给 rust-i18n
//! （`rust_i18n::set_locale`），使托盘菜单 / 系统通知按当前语言渲染。App 内文案由前端
//! Lingui 负责，二者不重叠。locale 目录见 `src-tauri/locales/`，`i18n!` 宏在 [`crate`] 根。
//!
//! 两个时机：
//! - 启动：[`init_locale_from_store`] 读持久化 locale（必须在 `build_tray` 之前）。
//! - 切换：[`crate::commands::set_locale`] 命令在用户改语言时调 [`set_locale`] + 重绘托盘。

use tauri::AppHandle;

/// 从持久化偏好读取 locale 并应用到 rust-i18n。读取失败（首启无偏好文件、格式不符等）
/// 保持 rust-i18n 默认（= `i18n!` 的 fallback `zh`）。**必须在 `build_tray` 之前调用**，
/// 保证托盘首帧即正确语言、不闪。
pub fn init_locale_from_store(app: &AppHandle) {
    if let Some(locale) = read_persisted_locale(app) {
        rust_i18n::set_locale(&locale);
    }
}

/// 应用新 locale（`set_locale` 命令在用户切换语言时调用）。
pub fn set_locale(locale: &str) {
    rust_i18n::set_locale(locale);
}

/// 读 `preferences.json` → `"preferences-store"` → `state.locale`。
///
/// 该值是 zustand persist 经 JSONStorage 序列化后的 **JSON 字符串**（双层编码：
/// tauri-store 里存的是字符串，字符串内容才是 `{ "state": { "locale": ... }, "version": n }`），
/// 故取出后需再 `from_str` 一次。任何环节缺失 / 格式不符都返回 `None`（交由调用方回退）。
fn read_persisted_locale(app: &AppHandle) -> Option<String> {
    use tauri_plugin_store::StoreExt;

    let store = app.store("preferences.json").ok()?;
    let raw = store.get("preferences-store")?;
    let serialized = raw.as_str()?;
    let parsed: serde_json::Value = serde_json::from_str(serialized).ok()?;
    let locale = parsed.get("state")?.get("locale")?.as_str()?;
    Some(locale.to_string())
}
