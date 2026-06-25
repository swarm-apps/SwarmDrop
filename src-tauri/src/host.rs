//! Desktop host adapters —— 实现 [`swarmdrop_core::host`] 中的各个 trait。
//!
//! - [`event_bus`] —— [`EventBus`](swarmdrop_core::host::EventBus)：把 CoreEvent 翻成 Tauri emit
//! - [`keychain`] —— [`KeychainProvider`](swarmdrop_core::host::KeychainProvider)：系统 keychain
//! - [`notifier`] —— [`Notifier`](swarmdrop_core::host::Notifier)：桌面通知
//! - [`update_installer`] —— [`UpdateInstaller`](swarmdrop_core::host::UpdateInstaller)：自动更新
//! - [`file_source`] + [`file_sink`] —— [`FileAccess`](swarmdrop_core::host::FileAccess)：
//!   读源文件 / 写接收文件，桌面端走本地路径
//!
//! 注：[`AppPaths`](swarmdrop_core::host::AppPaths) trait 桌面端目前不需要——
//! 接收目录由 user 在 acceptReceive 时显式传入（见 P0 修复），不再通过
//! AppPaths 推断默认下载目录。如未来要恢复"默认下载目录"功能再加 adapter。

pub mod device_config;
pub mod event_bus;
#[cfg(debug_assertions)]
pub mod file_keychain;
pub mod file_sink;
pub mod file_source;
pub mod keychain;
pub mod notifier;
pub mod update_installer;

/// 身份存储后端工厂：**debug build 用文件后端**（[`file_keychain`]，绕开 dev
/// 二进制 ad-hoc 签名无法访问 macOS login keychain 的限制 ——
/// `errSecInteractionNotAllowed` / "User interaction is not allowed"），
/// **release build 用系统 keychain**（[`keychain::DesktopKeychainProvider`]）。
///
/// cfg 分叉只发生在这里，所有 command 通过本函数取 provider，调用方零 cfg。
/// 返回 `Arc<dyn KeychainProvider>` 统一两个分支的静态类型；core 的
/// `identity::*` 函数签名是 `P: KeychainProvider + ?Sized`，用 `&*provider` 传入。
pub fn keychain_provider(
    app: &tauri::AppHandle,
) -> crate::AppResult<std::sync::Arc<dyn swarmdrop_core::host::KeychainProvider>> {
    #[cfg(debug_assertions)]
    {
        Ok(std::sync::Arc::new(
            file_keychain::FileKeychainProvider::new(app)?,
        ))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        Ok(std::sync::Arc::new(
            keychain::DesktopKeychainProvider::new()?
        ))
    }
}
