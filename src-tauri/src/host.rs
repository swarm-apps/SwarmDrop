//! Desktop host adapters —— 实现 [`swarmdrop_core::host`] 中的各个 trait。
//!
//! - [`event_bus`] —— [`EventBus`](swarmdrop_core::host::EventBus)：把 CoreEvent 翻成 Tauri emit
//! - [`keychain`] —— [`KeychainProvider`](swarmdrop_core::host::KeychainProvider)：系统 keychain
//! - [`notifier`] —— [`Notifier`](swarmdrop_core::host::Notifier)：桌面通知
//! - [`paths`] —— [`AppPaths`](swarmdrop_core::host::AppPaths)：应用数据目录
//! - [`update_installer`] —— [`UpdateInstaller`](swarmdrop_core::host::UpdateInstaller)：自动更新
//! - [`file_source`] + [`file_sink`] —— [`FileAccess`](swarmdrop_core::host::FileAccess)：
//!   读源文件 / 写接收文件，桌面端走本地路径

pub mod event_bus;
pub mod file_sink;
pub mod file_source;
pub mod keychain;
pub mod notifier;
pub mod paths;
pub mod update_installer;
