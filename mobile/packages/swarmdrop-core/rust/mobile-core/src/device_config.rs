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

use std::path::PathBuf;

/// 由 host 传入的数据目录算出 `device_config.json` 的落点。
///
/// 去掉 `file://` 前缀与尾部斜杠 —— 与 `app.rs` 的 `open_db` 同一套处理：expo 的
/// `Paths.document.uri` 是 URI 而非裸路径（形如 `file:///.../Documents/`），直接喂给
/// `std::path` 会得到一个名为 `file:` 的**相对**目录，写进去的名字下次启动读不回来
/// （表现为「改了名字，重启又变回去」）。这是移动端相对桌面唯一的真差异。
pub(crate) fn device_config_path(data_dir: &str) -> PathBuf {
    let dir = data_dir
        .strip_prefix("file://")
        .unwrap_or(data_dir)
        .trim_end_matches('/');
    PathBuf::from(dir).join("device_config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 读写行为的用例住在 `crates/host` 的 `device_config_file`；这里只钉移动端独有的
    /// 那一条 —— `Paths.document.uri` 的 `file://` 前缀必须剥掉，且落点是绝对路径。
    #[test]
    fn strips_file_scheme_prefix_and_trailing_slash() {
        assert_eq!(
            device_config_path("file:///var/mobile/Documents/"),
            PathBuf::from("/var/mobile/Documents/device_config.json")
        );
        // 裸路径（Android 的 `Paths.document.uri` 也可能没有 scheme）原样接受
        assert_eq!(
            device_config_path("/data/user/0/com.yexiyue.swarmdrop/files"),
            PathBuf::from("/data/user/0/com.yexiyue.swarmdrop/files/device_config.json")
        );
    }
}
