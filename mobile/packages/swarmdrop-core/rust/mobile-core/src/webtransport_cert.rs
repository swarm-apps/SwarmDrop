//! WebTransport 证书对的移动端落点 —— `data_dir/webtransport-cert.pem`，与桌面同名同格式。
//!
//! **只管路径**，读写实现是共享的
//! [`WebTransportFileCertificateStore`](swarmdrop_net::WebTransportFileCertificateStore)，
//! 与 [`device_config`](crate::device_config) 同一体例：那份实现里有三条容易写错且没有
//! 反馈回路的不变量（原子写 / `0600` / 读失败不降级），两端各写一遍就是两次写错的机会。
//!
//! # 为什么移动端也监听 WebTransport
//!
//! 「手机在 NAT 后，浏览器直连它走不通」只对**公网**成立。**局域网内**浏览器直连手机是走
//! 得通的 —— 那正是移动端已经在监听 webrtc-direct 的理由（`presets::Native` 里那条地址的
//! 注释写的就是「浏览器到原生端的局域网直连入口」）。WebTransport 在回环上是它的 4.5 倍，
//! 同一个场景没有理由只开慢的那条。
//!
//! **不走 `KeychainProvider`，因此不动 uniffi 契约。** 那个端口的方法都是「读一次就完」
//! 的形态（身份与 webrtc 证书永不改变），而这份证书要 14 天轮换并**回写**。落在 Rust 侧
//! 的文件里，跨 FFI 面一个字节都没变。
//!
//! ⚠️ 落点是**应用私有数据区**（`data_dir`，与 `swarmdrop.db` 同级），不是用户可见的接收
//! 区 —— 它是密钥材料，不该出现在文件管理器里。两个目录的角色分离见 `CLAUDE.md` 的
//! 「接收落点恒为用户可见位置」。

use std::path::{Path, PathBuf};

/// 由数据目录算出 `webtransport-cert.pem` 的落点。
///
/// 入参已是文件系统路径 —— `file://` URI 的解析在 [`MobileCore::new`] 的边界由
/// [`crate::utils::parse_host_dir`] 一次性完成，本函数只负责拼接。
///
/// [`MobileCore::new`]: crate::app::MobileCore::new
pub(crate) fn cert_path(data_dir: &Path) -> PathBuf {
    data_dir.join("webtransport-cert.pem")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 读写行为的用例住在 `crates/net` 的 `cert_store`；这里只钉落点。
    #[test]
    fn appends_cert_filename_to_data_dir() {
        assert_eq!(
            cert_path(Path::new("/var/mobile/Library/Application Support")),
            PathBuf::from("/var/mobile/Library/Application Support/webtransport-cert.pem")
        );
        assert_eq!(
            cert_path(Path::new("/data/user/0/com.yexiyue.swarmdrop/files")),
            PathBuf::from("/data/user/0/com.yexiyue.swarmdrop/files/webtransport-cert.pem")
        );
    }
}
