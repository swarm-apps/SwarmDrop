//! 身份和已配对设备持久化逻辑。

use swarm_p2p_core::libp2p::{identity::Keypair, PeerId};

use crate::device::PairedDeviceInfo;
use crate::error::{AppError, AppResult};
use crate::host::{DeviceIdentityBytes, IdentityMigrationState, KeychainProvider};

/// 已初始化的设备身份。
pub struct InitializedIdentity {
    pub keypair: Keypair,
    pub keypair_bytes: Vec<u8>,
    pub peer_id: PeerId,
    pub created: bool,
}

/// 从 host keychain 读取设备身份；不存在时自动生成并保存。
pub async fn load_or_create_identity<P>(provider: &P) -> AppResult<InitializedIdentity>
where
    P: KeychainProvider + ?Sized,
{
    if let Some(identity) = provider.load_identity().await? {
        let keypair = Keypair::from_protobuf_encoding(&identity.keypair)
            .map_err(|error| AppError::Identity(error.to_string()))?;
        let peer_id = PeerId::from_public_key(&keypair.public());

        return Ok(InitializedIdentity {
            keypair,
            keypair_bytes: identity.keypair,
            peer_id,
            created: false,
        });
    }

    let keypair = Keypair::generate_ed25519();
    let keypair_bytes = keypair
        .to_protobuf_encoding()
        .map_err(|error| AppError::Identity(error.to_string()))?;
    let peer_id = PeerId::from_public_key(&keypair.public());

    provider
        .save_identity(DeviceIdentityBytes {
            keypair: keypair_bytes.clone(),
        })
        .await?;
    provider
        .save_migration_state(IdentityMigrationState::Completed)
        .await?;

    Ok(InitializedIdentity {
        keypair,
        keypair_bytes,
        peer_id,
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
        *existing = device;
    } else {
        devices.push(device);
    }
    provider.save_paired_devices(devices.clone()).await?;
    Ok(devices)
}

/// 移除一个已配对设备，并返回更新后的列表。
pub async fn remove_paired_device<P>(
    provider: &P,
    peer_id: &PeerId,
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

    use swarm_p2p_core::libp2p::identity::Keypair;
    use swarm_p2p_core::libp2p::PeerId;

    use crate::device::{OsInfo, PairedDeviceInfo};
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
        let keypair = Keypair::generate_ed25519();
        PairedDeviceInfo {
            peer_id: PeerId::from_public_key(&keypair.public()),
            os_info: OsInfo {
                name: None,
                hostname: name.to_string(),
                os: "test".to_string(),
                platform: "test".to_string(),
                arch: "test".to_string(),
            },
            paired_at: 1,
        }
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
        let devices = super::upsert_paired_device(&host, device).await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].peer_id, peer_id);
        assert_eq!(devices[0].os_info.hostname, "second");
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
        assert_eq!(created.peer_id, loaded.peer_id);
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
