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

pub mod event_bus;
pub mod file_sink;
pub mod file_source;
pub mod keychain;
pub mod notifier;
pub mod update_installer;
