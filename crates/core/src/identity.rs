//! 身份和已配对设备持久化逻辑。

use swarmdrop_net::{NodeId, SecretKey};

use crate::device::{DeviceReceivePolicy, DeviceTrustLevel, PairedDeviceInfo};
use crate::error::{AppError, AppResult};
use crate::host::{DeviceIdentityBytes, IdentityMigrationState, KeychainProvider};

/// 已初始化的设备身份。
pub struct InitializedIdentity {
    pub secret_key: SecretKey,
    pub keypair_bytes: Vec<u8>,
    pub node_id: NodeId,
    pub created: bool,
}

/// 从 host keychain 读取设备身份；不存在时自动生成并保存。
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
    provider
        .save_migration_state(IdentityMigrationState::Completed)
        .await?;

    Ok(InitializedIdentity {
        secret_key,
        keypair_bytes,
        node_id,
        created: true,
    })
}

/// 读取已配对设备列表。
pub async fn load_paired_devices<P>(provider: &P) -> AppResult<Vec<PairedDeviceInfo>>
where
    P: KeychainProvider + ?Sized,
{
    provider.load_paired_devices().await
}

/// 覆盖保存已配对设备列表。
pub async fn save_paired_devices<P>(provider: &P, devices: Vec<PairedDeviceInfo>) -> AppResult<()>
where
    P: KeychainProvider + ?Sized,
{
    provider.save_paired_devices(devices).await
}

/// 添加或替换一个已配对设备，并返回更新后的列表。
pub async fn upsert_paired_device<P>(
    provider: &P,
    device: PairedDeviceInfo,
) -> AppResult<Vec<PairedDeviceInfo>>
where
    P: KeychainProvider + ?Sized,
{
    let mut devices = provider.load_paired_devices().await?;
    if let Some(existing) = devices
        .iter_mut()
        .find(|item| item.peer_id == device.peer_id)
    {
        existing.os_info = device.os_info;
        existing.paired_at = device.paired_at;
    } else {
        devices.push(device);
    }
    provider.save_paired_devices(devices.clone()).await?;
    Ok(devices)
}

/// 更新已配对设备的可信策略，并返回更新后的列表。
pub async fn update_paired_device_policy<P>(
    provider: &P,
    peer_id: &NodeId,
    trust_level: DeviceTrustLevel,
    receive_policy: Option<DeviceReceivePolicy>,
) -> AppResult<Vec<PairedDeviceInfo>>
where
    P: KeychainProvider + ?Sized,
{
    let mut devices = provider.load_paired_devices().await?;
    let Some(device) = devices.iter_mut().find(|item| &item.peer_id == peer_id) else {
        return Err(AppError::Identity("未找到已配对设备".to_string()));
    };

    device.trust_level = trust_level;
    device.receive_policy =
        receive_policy.unwrap_or_else(|| DeviceReceivePolicy::for_trust_level(trust_level));
    device.trust_confirmed = true;

    provider.save_paired_devices(devices.clone()).await?;
    Ok(devices)
}

/// 移除一个已配对设备，并返回更新后的列表。
pub async fn remove_paired_device<P>(
    provider: &P,
    peer_id: &NodeId,
) -> AppResult<Vec<PairedDeviceInfo>>
where
    P: KeychainProvider + ?Sized,
{
    let mut devices = provider.load_paired_devices().await?;
    devices.retain(|item| &item.peer_id != peer_id);
    provider.save_paired_devices(devices.clone()).await?;
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use swarmdrop_net::SecretKey;

    use crate::device::{DeviceTrustLevel, OsInfo, PairedDeviceInfo};
    use crate::host::{CoreAppPaths, KeychainProvider, MemoryHost};

    fn memory_host() -> MemoryHost {
        MemoryHost::new(CoreAppPaths {
            data_dir: PathBuf::from("data"),
            cache_dir: PathBuf::from("cache"),
            temp_dir: PathBuf::from("temp"),
            log_dir: PathBuf::from("log"),
        })
    }

    fn paired_device(name: &str) -> PairedDeviceInfo {
        PairedDeviceInfo::new(
            SecretKey::generate().node_id(),
            OsInfo {
                name: None,
                hostname: name.to_string(),
                os: "test".to_string(),
                platform: "test".to_string(),
                arch: "test".to_string(),
                capabilities: Vec::new(),
            },
            1,
        )
    }

    #[tokio::test]
    async fn upsert_paired_device_should_insert_then_replace() {
        let host = memory_host();
        let mut device = paired_device("first");
        let peer_id = device.peer_id;

        let devices = super::upsert_paired_device(&host, device.clone())
            .await
            .unwrap();
        assert_eq!(devices.len(), 1);

        device.os_info.hostname = "second".to_string();
        device.trust_level = DeviceTrustLevel::Owned;
        let devices = super::upsert_paired_device(&host, device).await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].peer_id, peer_id);
        assert_eq!(devices[0].os_info.hostname, "second");
        assert_eq!(devices[0].trust_level, DeviceTrustLevel::Collaborator);
    }

    #[tokio::test]
    async fn update_paired_device_policy_should_confirm_trust() {
        let host = memory_host();
        let device = paired_device("first");
        let peer_id = device.peer_id;
        host.save_paired_devices(vec![device]).await.unwrap();

        let devices =
            super::update_paired_device_policy(&host, &peer_id, DeviceTrustLevel::Owned, None)
                .await
                .unwrap();

        assert_eq!(devices[0].trust_level, DeviceTrustLevel::Owned);
        assert!(devices[0].receive_policy.auto_accept);
        assert!(devices[0].trust_confirmed);
    }

    #[tokio::test]
    async fn load_or_create_identity_should_create_then_reuse_keypair() {
        let host = memory_host();

        let created = super::load_or_create_identity(&host).await.unwrap();
        assert!(created.created);
        assert_eq!(
            host.load_migration_state().await.unwrap(),
            crate::host::IdentityMigrationState::Completed
        );

        let loaded = super::load_or_create_identity(&host).await.unwrap();
        assert!(!loaded.created);
        assert_eq!(created.node_id, loaded.node_id);
        assert_eq!(created.keypair_bytes, loaded.keypair_bytes);
    }

    #[tokio::test]
    async fn remove_paired_device_should_persist_filtered_list() {
        let host = memory_host();
        let first = paired_device("first");
        let second = paired_device("second");
        let first_peer = first.peer_id;

        host.save_paired_devices(vec![first, second]).await.unwrap();

        let devices = super::remove_paired_device(&host, &first_peer)
            .await
            .unwrap();
        assert_eq!(devices.len(), 1);
        assert_ne!(devices[0].peer_id, first_peer);
    }
}
