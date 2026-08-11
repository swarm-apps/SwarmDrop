//! 设备密钥材料的持久化逻辑：Ed25519 身份与 WebRTC Direct 证书。
//!
//! **只管密钥**——已配对设备列表是可导出的业务数据，走另一个端口，见
//! [`crate::paired_devices`]。

use swarmdrop_net::{NodeId, SecretKey};

use crate::error::{AppError, AppResult};
use crate::host::{DeviceIdentityBytes, KeychainProvider};

/// 已初始化的设备身份。
pub struct InitializedIdentity {
    pub secret_key: SecretKey,
    pub keypair_bytes: Vec<u8>,
    pub node_id: NodeId,
    pub created: bool,
}

/// 从宿主身份存储读取设备身份；不存在时自动生成并保存。
///
/// keypair 存量为 protobuf 编码，[`SecretKey::from_protobuf`] 与之完全兼容。
pub async fn load_or_create_identity<P>(provider: &P) -> AppResult<InitializedIdentity>
where
    P: KeychainProvider + ?Sized,
{
    if let Some(identity) = provider.load_identity().await? {
        let secret_key = SecretKey::from_protobuf(&identity.keypair)
            .map_err(|error| AppError::Identity(error.to_string()))?;
        let node_id = secret_key.node_id();

        return Ok(InitializedIdentity {
            secret_key,
            keypair_bytes: identity.keypair,
            node_id,
            created: false,
        });
    }

    let secret_key = SecretKey::generate();
    let keypair_bytes = secret_key.to_protobuf();
    let node_id = secret_key.node_id();

    provider
        .save_identity(DeviceIdentityBytes {
            keypair: keypair_bytes.clone(),
        })
        .await?;

    Ok(InitializedIdentity {
        secret_key,
        keypair_bytes,
        node_id,
        created: true,
    })
}

/// 读取或首次生成 WebRTC Direct 证书。
///
/// 证书 PEM 必须整体持久化；仅重建密钥仍会因证书中的随机字段改变 certhash，
/// 导致已经发出的邀请地址失效。此函数仅在原生宿主编译。
#[cfg(not(target_family = "wasm"))]
pub async fn load_or_create_webrtc_certificate<P>(provider: &P) -> AppResult<String>
where
    P: KeychainProvider + ?Sized,
{
    if let Some(pem) = provider.load_webrtc_certificate_pem().await? {
        return Ok(pem);
    }

    let pem = swarmdrop_net::generate_webrtc_certificate_pem().map_err(AppError::Network)?;
    provider.save_webrtc_certificate_pem(pem.clone()).await?;
    Ok(pem)
}

#[cfg(test)]
mod tests {
    use crate::host::MemoryHost;

    fn memory_host() -> MemoryHost {
        MemoryHost::new()
    }

    #[tokio::test]
    async fn load_or_create_identity_should_create_then_reuse_keypair() {
        let host = memory_host();

        let created = super::load_or_create_identity(&host).await.unwrap();
        assert!(created.created);

        let loaded = super::load_or_create_identity(&host).await.unwrap();
        assert!(!loaded.created);
        assert_eq!(created.node_id, loaded.node_id);
        assert_eq!(created.keypair_bytes, loaded.keypair_bytes);
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn load_or_create_webrtc_certificate_should_reuse_pem() {
        let host = memory_host();

        let created = super::load_or_create_webrtc_certificate(&host)
            .await
            .expect("create certificate");
        let loaded = super::load_or_create_webrtc_certificate(&host)
            .await
            .expect("reuse certificate");

        assert_eq!(created, loaded);
    }
}
