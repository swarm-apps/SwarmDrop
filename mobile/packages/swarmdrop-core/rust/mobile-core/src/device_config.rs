//! 设备名持久化的**路径解析** —— `data_dir/device_config.json`，与桌面端同名同格式。
//!
//! 与敏感数据（identity / paired_devices 走 keychain）区分开：设备名是用户在
//! onboarding / 设置里起的名字，无加密需求，一个普通 JSON 文件即可。
//!
//! **为什么落在 Rust 侧而不是回跨 FFI 问 JS 的 AsyncStorage。** 后者只是把「移动端
//! 设备名归 JS 管」换个更正式的写法，Rust 侧依然要靠外部喂；而落盘在这里之后，三端
//! 的实现体量都收敛成「一个文件 / 一个 KV，读写 + 降级」，[`DeviceConfig`] 这个端口
//! 才真的在抹平差异。代价是存量安装需要一次性迁移（JS bootstrap 里做，见
//! `mobile/src/stores/mobile-core-store.ts`）。
//!
//! **读写实现不在这里**：它是共享的
//! [`JsonFileDeviceConfig`](swarmdrop_core::host::JsonFileDeviceConfig)。移动端与桌面
//! 此前各有一份逐行同构的实现（同一个 `{ "device_name": … }` 结构、同一套容错读、同一个
//! 建目录 + pretty 写、连 load 侧再过一次 `DeviceName::parse` 的防线都一样），两者唯一的
//! 真差异就是本模块剩下的这件事 —— 路径从哪来。
//!
//! [`DeviceConfig`]: swarmdrop_core::host::DeviceConfig

use std::path::{Path, PathBuf};

/// 由数据目录算出 `device_config.json` 的落点。
///
/// 入参已是文件系统路径——`file://` URI 的解析在 [`MobileCore::new`] 的边界由
/// [`crate::utils::parse_host_dir`] 一次性完成，本函数只负责拼接。
///
/// [`MobileCore::new`]: crate::app::MobileCore::new
pub(crate) fn device_config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("device_config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 读写行为的用例住在 `crates/host` 的 `device_config_file`；这里只钉落点。
    /// URI 解析已由 `utils::parse_host_dir` 的用例覆盖。
    #[test]
    fn appends_config_filename_to_data_dir() {
        assert_eq!(
            device_config_path(Path::new("/var/mobile/Documents")),
            PathBuf::from("/var/mobile/Documents/device_config.json")
        );
        assert_eq!(
            device_config_path(Path::new("/data/user/0/com.yexiyue.swarmdrop/files")),
            PathBuf::from("/data/user/0/com.yexiyue.swarmdrop/files/device_config.json")
        );
    }
}
