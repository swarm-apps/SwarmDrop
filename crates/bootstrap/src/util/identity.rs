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

fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("创建密钥目录失败")?;
    }
    fs::write(path, contents).context("写入私钥文件失败")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .context("设置私钥文件权限失败")?;
    }
    Ok(())
}
