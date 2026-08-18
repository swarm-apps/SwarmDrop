//! 桌面端设备身份与已配对设备列表的**存放位置**。
//!
//! 读写实现是端口层共享的 [`JsonFileIdentityStore`]（桌面与命令行宿主逐行同构的那一份，
//! 判据、安全形态与三条不可协商的保证都写在它的模块文档里，此处不复述——复述过的注释
//! 只会各自漂移）。本模块只回答桌面特有的那一半：**目录从哪来**。
//!
//! ## 为什么不用 keychain
//!
//! 本应用的 macOS 签名标识是 `"-"`（ad-hoc，见 `tauri.conf.json`），没有稳定的
//! designated requirement——cdhash 每次构建都变。macOS 的 keychain ACL 按 DR 匹配调用方，
//! 于是每次启动都认不出是同一个应用；也正因为没有稳定标识，系统无法把它写进 item 的可信
//! 应用列表，「始终允许」形同虚设。启动读三条 item 就弹三次。
//!
//! debug build 此前已经走文件后端（同一个签名问题，只是失败形态是 `errSecInteractionNotAllowed`
//! 而非弹框），release 走 keychain——两个构建面对同一个问题，只有 debug 那侧承认了它。
//!
//! per-app ACL 只有 macOS 有：Windows 凭据管理器按用户账户隔离、同用户下任何进程都能读，
//! Linux Secret Service 在 keyring 锁定时才加密。三平台统一走文件避免了一个永久的
//! `cfg(target_os)` 存储后端分叉。
//!
//! 将来若拿到 Developer ID 签名要切回 keychain，是换一个端口实现——本模块与共享实现
//! 都不需要为此保留任何钩子。

use swarmdrop_core::error::{AppError as CoreError, AppResult as CoreResult};
use swarmdrop_host_fs::JsonFileIdentityStore;

/// 构造桌面端的身份存储。
///
/// **本机数据目录，不是漫游目录**：Windows 上 `app_data_dir` 是 `%APPDATA%`，会被域漫游
/// 配置文件同步到服务器——私钥不该跟着漫游。macOS 与 Linux 上两者解析到同一个目录，
/// 所以这不是平台分叉，只是在 Windows 上恰好落到了对的那个。
pub fn new(app: &tauri::AppHandle) -> CoreResult<JsonFileIdentityStore> {
    let dir = crate::host::paths::app_local_data_dir(app)
        .map_err(|e| CoreError::Identity(format!("identity store: 数据目录不可用: {e}")))?;
    Ok(JsonFileIdentityStore::new(dir))
}
