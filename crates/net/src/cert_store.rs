//! [`WebTransportCertificateStore`] 的文件实现，桌面与移动端共用。
//!
//! # 为什么共享一份，而不是各端各写
//!
//! 本仓的端口实现多数是三端各写一份（`PairedDeviceStore` 就是），因为它们的差异本来就在
//! 平台。这一个不同：它的三条不变量都**容易写错、且错了没有反馈回路**——
//!
//! - **原子写**：证书是周期性重写同一个文件，写到一半掉电会留下半截 PEM。下次启动解析
//!   失败 → 重新生成 → certhash 变 → 对端记下的地址全部失效，而本机日志一切正常。
//! - **`0600`**：文件里有私钥。
//! - **读失败不得降级成「还没有」**：降级会让一次瞬时 IO 故障变成「生成新证书并覆盖原
//!   文件」，一次坏块就永久换掉身份。
//!
//! 三条各写两遍就是两次写错的机会。路径仍由各宿主给（桌面 `app_local_data_dir`、
//! 移动端 `data_dir`），与 [`JsonFileDeviceConfig`] 同一体例。
//!
//! [`JsonFileDeviceConfig`]: https://docs.rs/swarmdrop-host

use std::path::{Path, PathBuf};

use webtransport_p2p::{CertificateStore, StoreError};

/// 证书对的文件存储。
///
/// ⚠️ **文件名由调用方给全路径**，本类型不拼接——两端的目录语义不同（桌面是
/// `app_local_data_dir`，移动端是 uniffi 边界解析过的 `data_dir`），在这里猜文件名只会
/// 让「证书存哪」这件事有两个事实源。
#[derive(Debug, Clone)]
pub struct WebTransportFileCertificateStore {
    path: PathBuf,
}

impl WebTransportFileCertificateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl CertificateStore for WebTransportFileCertificateStore {
    fn load(&self) -> Result<Option<String>, StoreError> {
        match std::fs::read_to_string(&self.path) {
            Ok(pem) => Ok(Some(pem)),
            // 首启的正常路径，不是错误。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            // ⚠️ 读取失败**不得**降级成 `Ok(None)`：那会让一次瞬时 IO 故障（权限、坏块）
            // 被当成「还没有证书」，内核随即生成一对新的**并覆盖原文件**——一次读失败就
            // 永久换掉了 certhash，用户只看到「浏览器突然连不上这台设备了」。
            Err(e) => Err(StoreError::new(format!(
                "读取 {} 失败: {e}",
                self.path.display()
            ))),
        }
    }

    fn store(&self, pem: &str) -> Result<(), StoreError> {
        write_private_atomic(&self.path, pem).map_err(StoreError::new)
    }
}

/// 原子写一份私密文本：同目录临时文件 → fsync → 原子替换 → fsync 父目录。
///
/// - **同目录**：跨文件系统的 `persist` 会失败（`EXDEV`）。
/// - **`sync_all` 在替换之前**：rename 的原子性只覆盖**元数据**，ext4/xfs 上完全可能出现
///   「替换生效了、数据块还没落盘」，崩溃后目标是零长度或垃圾——正是这段代码要防的形态。
/// - **权限、随机名、`O_EXCL`、异常路径清残留**交给 `tempfile`（它默认就给 unix `0600`）。
///   自己搓这四条最容易漏的是最后一条：提前返回时留下的临时文件里是私钥。
/// - **父目录也 fsync**，否则 rename 这条目录项本身可能没落盘：崩溃后目标路径退回**上一轮**
///   的证书对，而那时 `current` 可能已接近过期，表现为一段时间谁都拨不进来。失败不致命
///   （文件内容已经 sync 过，最坏是丢掉这次替换），故忽略错误。
///
/// 与身份文件那条（`src-tauri` 的 `identity_store`）的差别是**刻意的**：那边写的是
/// `identity.json`，丢掉最后一次写只意味着设备名之类退回上一版；这份丢掉则可能撞上过期
/// 窗口，所以这里恒 fsync、不给 `Durability` 选项。
fn write_private_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;

    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} 没有父目录", path.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;

    let mut tmp = tempfile::Builder::new()
        .suffix(".tmp")
        .tempfile_in(parent)?;
    tmp.write_all(text.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;

    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "webtransport-cert.pem";

    fn store_at(dir: &Path) -> WebTransportFileCertificateStore {
        WebTransportFileCertificateStore::new(dir.join(NAME))
    }

    /// 首启：文件不存在是正常路径，返回 `Ok(None)` 而不是错误。
    #[test]
    fn missing_file_is_first_launch_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(store_at(dir.path()).load().expect("load").is_none());
    }

    /// 多段 PEM 原样往返 —— 两张证书连着私钥都在里面，任何一侧改写都会让它失效。
    #[test]
    fn roundtrips_multi_section_pem() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_at(dir.path());
        let pem = "-----BEGIN A-----\naaa\n-----END A-----\n\
                   -----BEGIN B-----\nbbb\n-----END B-----\n";

        store.store(pem).expect("store");

        assert_eq!(store.load().expect("load").as_deref(), Some(pem));
    }

    /// **护栏：读失败必须是 `Err`，不得降级成 `Ok(None)`。**
    ///
    /// 用「路径是个目录」制造一个非 `NotFound` 的读错误——比改权限稳，CI 的 root 容器里
    /// 也照样复现。
    #[test]
    fn unreadable_file_is_an_error_not_a_fresh_start() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join(NAME)).expect("造一个同名目录");

        assert!(
            store_at(dir.path()).load().is_err(),
            "读不出来时必须报错——降级成 Ok(None) 会让内核覆盖掉还好好的证书"
        );
    }

    /// 覆写走原子替换，且不留临时文件残留（残留里有私钥）。
    #[test]
    fn overwrite_is_atomic_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_at(dir.path());

        store.store("first").expect("first write");
        store.store("second").expect("second write");

        assert_eq!(store.load().expect("load").as_deref(), Some("second"));
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != NAME)
            .collect();
        assert!(leftovers.is_empty(), "不应留下临时文件：{leftovers:?}");
    }

    /// 私钥文件必须是 `0600` —— 防的是同机其他用户。
    #[cfg(unix)]
    #[test]
    fn written_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_at(dir.path());
        store.store("secret").expect("store");

        let mode = std::fs::metadata(dir.path().join(NAME))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "证书文件里有私钥，必须是 0600");
    }

    /// 父目录不存在时自行创建 —— 移动端首启时 `data_dir` 可能还是空的。
    #[test]
    fn creates_missing_parent_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("a").join("b");
        let store = store_at(&nested);

        store.store("pem").expect("store");

        assert_eq!(store.load().expect("load").as_deref(), Some("pem"));
    }
}
