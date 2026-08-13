use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use swarmdrop_net::{SecretKey, generate_webrtc_certificate_pem};
use tracing::info;

/// 加载或生成 Ed25519 身份；protobuf 格式与旧 bootstrap、客户端完全兼容。
pub fn load_or_generate_secret_key(path: &Path) -> Result<SecretKey> {
    if path.exists() {
        info!(path = %path.display(), "加载节点身份");
        let bytes = fs::read(path).context("读取节点身份文件失败")?;
        return SecretKey::from_protobuf(&bytes).context("节点身份文件不是有效的 Ed25519 protobuf");
    }

    info!(path = %path.display(), "生成新的 Ed25519 节点身份");
    let secret = SecretKey::generate();
    write_private_file(path, &secret.to_protobuf()).context("保存节点身份文件失败")?;
    Ok(secret)
}

/// 加载或生成完整 webrtc-direct PEM（含私钥），保证 certhash 跨重启稳定。
pub fn load_or_generate_webrtc_certificate(path: &Path) -> Result<String> {
    if path.exists() {
        info!(path = %path.display(), "加载持久化 WebRTC Direct 证书");
        let pem = fs::read_to_string(path).context("读取 WebRTC Direct 证书失败")?;
        if pem.trim().is_empty() {
            bail!("WebRTC Direct 证书文件为空: {}", path.display());
        }
        return Ok(pem);
    }

    info!(path = %path.display(), "生成持久化 WebRTC Direct 证书");
    let pem = generate_webrtc_certificate_pem().map_err(anyhow::Error::msg)?;
    write_private_file(path, pem.as_bytes()).context("保存 WebRTC Direct 证书失败")?;
    Ok(pem)
}

/// 原子写私钥类文件：临时文件 → 权限 → fsync → rename → fsync 父目录。
///
/// **不能直接 `fs::write`**：证书轮换是周期性重写同一个文件，写到一半掉电就会留下一个
/// 半截 PEM —— 下次启动解析失败 → 重新生成 → certhash 变 → 对端记下的地址全部失效。
///
/// 三个细节都不能省：
///
/// - **`sync_all` 在 rename 之前。** rename 的原子性只覆盖**元数据**：ext4/xfs 上完全可能
///   出现「rename 生效了、但数据块还没落盘」，崩溃后目标路径是零长度或垃圾 —— 正是这段
///   代码声称要防的那个形态。
/// - **父目录也要 fsync**，否则 rename 本身可能没落盘。
/// - **临时名是追加 `.tmp` 而不是替换扩展名。** `with_extension("tmp")` 遇上本身就叫
///   `foo.tmp` 的目标路径会算出同一个名字，退化成原地写 + `rename(p, p)`：**静默地**
///   失去原子性。
///
/// 任一步失败都要清掉临时文件 —— 它里面是私钥，而 `set_permissions` 之前它还是 umask 默认
/// 的 0644。
fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("创建密钥目录失败")?;
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("私钥路径没有文件名")?;
    let tmp = path.with_file_name(format!("{file_name}.tmp"));

    let result = (|| -> Result<()> {
        let mut file = fs::File::create(&tmp).context("创建临时私钥文件失败")?;
        file.write_all(contents).context("写入临时私钥文件失败")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // 权限要在 rename **之前**设好，否则存在一个短暂的 0644 窗口。
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
                .context("设置私钥文件权限失败")?;
        }
        file.sync_all().context("刷新临时私钥文件到磁盘失败")?;
        Ok(())
    })();

    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow::Error::from(e).context("原子替换私钥文件失败"));
    }

    // 让 rename 这条目录项本身落盘。失败不致命（文件内容已经 sync 过），只记不报。
    if let Some(parent) = path.parent()
        && let Ok(dir) = fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
    Ok(())
}

// WebTransport 证书对的存储**不在这里**：用 `swarmdrop_net::WebTransportFileCertificateStore`
// （与桌面、移动端同一份实现）。
//
// 这里曾有一份 bootstrap 专用的拷贝，它的 `load` 用 `Path::exists()` 判首启 —— 而
// `exists()` 在权限失败 / EIO / 父目录不可遍历时**一律返回 false**，于是读失败被降级成
// 「还没有证书」，内核随即生成一对新的并覆盖原文件。对 bootstrap 尤其致命：它是所有
// 浏览器的入口，certhash 一变全网都拨不通 4004，而日志里什么都看不到。
//
// 共享那份用 `ErrorKind::NotFound` 精确匹配，并有一条 `unreadable_file_is_an_error_not_a_fresh_start`
// 护栏测试看守这条判据。
