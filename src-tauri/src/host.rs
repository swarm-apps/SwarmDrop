//! Desktop host adapters —— 实现 [`swarmdrop_core::host`] 中的各个 trait。
//!
//! - [`event_bus`] —— [`EventBus`](swarmdrop_core::host::EventBus)：把 CoreEvent 翻成 Tauri emit
//! - [`keychain`] —— [`KeychainProvider`](swarmdrop_core::host::KeychainProvider)：系统 keychain，
//!   同时实现 [`PairedDeviceStore`](swarmdrop_core::host::PairedDeviceStore)：已配对设备列表
//! - [`device_config`] —— [`DeviceConfig`](swarmdrop_core::host::DeviceConfig)：用户设备名，
//!   实现体是端口层共享的 [`JsonFileDeviceConfig`]，桌面只算路径
//! - [`notifier`] —— [`Notifier`](swarmdrop_core::host::Notifier)：桌面通知
//! - [`update_installer`] —— [`UpdateInstaller`](swarmdrop_core::host::UpdateInstaller)：自动更新
//! - [`file_source`] + [`file_sink`] —— [`FileAccess`](swarmdrop_core::host::FileAccess)：
//!   读源文件 / 写接收文件，桌面端走本地路径
//!
//! [`paths`] 不是端口，是上面几个 adapter 共用的数据目录解析（含 `SWARMDROP_DATA_DIR`
//! 的 debug-only fixture 覆盖）。
//!
//! 注：端口层原有的 `AppPaths` trait 已删除（零生产实现、零生产消费）——接收目录由
//! user 在 acceptReceive 时显式传入，不再由端口推断默认下载目录。如未来要恢复
//! "默认下载目录"功能，重建那个端口比留一个只有测试替身实现的空壳便宜。

use std::sync::Arc;

use swarmdrop_core::host::{JsonFileDeviceConfig, KeychainProvider, PairedDeviceStore};

pub mod event_bus;
#[cfg(debug_assertions)]
pub mod file_keychain;
pub mod file_sink;
pub mod file_source;
pub mod keychain;
pub mod notifier;
// fixture 覆盖本身是 `cfg(debug_assertions)` 门控的（见模块内 `fixture_dir`），
// 所以模块声明**不加** cfg —— release 下它照常编译，只是永远走平台默认目录。
pub mod paths;
pub mod update_installer;

/// 身份与已配对设备的桌面存储后端：**debug build 用文件后端**（[`file_keychain`]，
/// 绕开 dev 二进制 ad-hoc 签名无法访问 macOS login keychain 的限制 ——
/// `errSecInteractionNotAllowed` / "User interaction is not allowed"），
/// **release build 用系统 keychain**（[`keychain::DesktopKeychainProvider`]）。
///
/// **cfg 判别只在这个别名与紧随其后的构造函数上**。两个端口工厂
/// （[`keychain_provider`] / [`paired_device_store`]）因此零 cfg —— 它们此前各写一份
/// 逐字相同的分叉，漏改一处就是「密钥走文件、设备列表走 keychain」：只在 debug 出现、
/// 极难联想的错配。收进这里之后，两个分支不一致会直接编译不过。
#[cfg(debug_assertions)]
type DesktopSecretStore = file_keychain::FileKeychainProvider;
#[cfg(not(debug_assertions))]
type DesktopSecretStore = keychain::DesktopKeychainProvider;

#[cfg(debug_assertions)]
fn secret_store(app: &tauri::AppHandle) -> crate::AppResult<Arc<DesktopSecretStore>> {
    Ok(Arc::new(file_keychain::FileKeychainProvider::new(app)?))
}

#[cfg(not(debug_assertions))]
fn secret_store(_app: &tauri::AppHandle) -> crate::AppResult<Arc<DesktopSecretStore>> {
    Ok(Arc::new(keychain::DesktopKeychainProvider::new()?))
}

/// 身份存储端口。
///
/// core 的 `identity::*` 函数签名是 `P: KeychainProvider + ?Sized`，用 `&*provider` 传入。
pub fn keychain_provider(app: &tauri::AppHandle) -> crate::AppResult<Arc<dyn KeychainProvider>> {
    let store: Arc<dyn KeychainProvider> = secret_store(app)?;
    Ok(store)
}

/// 已配对设备列表的存储端口。
///
/// 桌面端两个端口由同一个结构体实现，但**取用要分开**：设备列表是可导出、会整份覆写的
/// 业务数据，密钥材料是不出进程的秘密（见
/// [`PairedDeviceStore`](swarmdrop_core::host::PairedDeviceStore) 的文档）。core 的
/// `paired_devices::*` 函数签名收 `&dyn PairedDeviceStore`，用 `&*store` 传入。
pub fn paired_device_store(app: &tauri::AppHandle) -> crate::AppResult<Arc<dyn PairedDeviceStore>> {
    let store: Arc<dyn PairedDeviceStore> = secret_store(app)?;
    Ok(store)
}

/// 设备名配置端口：`app_data_dir/device_config.json`。
///
/// 实现体是端口层共享的 [`JsonFileDeviceConfig`] —— 桌面与移动此前是逐行同构的两份
/// （同一个 JSON 结构、同一套容错读、同一条「读出串再过一次 `DeviceName::parse`」的防线），
/// 真差异只有**路径从哪来**。桌面这侧于是只剩下面这一行。
///
/// 路径解析失败（app_data_dir 不可用 = 安装损坏）现在直接冒泡而非降级成「读不到名字」：
/// 那种降级会让「改了名字、重启又变回去」表现成静默的怪事。
pub fn device_config(app: &tauri::AppHandle) -> crate::AppResult<JsonFileDeviceConfig> {
    Ok(JsonFileDeviceConfig::new(
        paths::app_data_dir(app)?.join("device_config.json"),
    ))
}
