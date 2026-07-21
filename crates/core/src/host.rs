//! Host 能力聚合。
//!
//! 端口层 trait + DTO（`FileAccess` / `KeychainProvider` / `CoreSaveLocation` …）已下沉到
//! [`swarmdrop_host`]，本模块 glob 再导出以保持 `crate::host::*` 路径不变。
//!
//! 留在 core 的是**聚合层**：[`CoreEvent`] 汇聚 network / pairing / transfer 各域事件
//! （且反向引用 transfer wire 类型），[`EventBus`] 以它为消息，[`MemoryHost`] 为测试替身。
//! 三者引用上层类型，无法随端口层下沉——否则 host ↔ transfer 成环。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use swarmdrop_net::NodeId;

pub use swarmdrop_host::*;

use swarmdrop_host::device::{Device, PairedDeviceInfo};
use swarmdrop_transfer::incoming::TransferOfferEvent;
use swarmdrop_transfer::progress::{
    PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent, TransferDbErrorEvent,
    TransferFailedEvent, TransferPausedEvent, TransferProgressEvent, TransferRejectedEvent,
    TransferResumedEvent,
};
use swarmdrop_transfer::store::TransferProjection;

use crate::network::NetworkStatus;
use crate::protocol::PairingRequest;

/// Core 事件。
///
/// `#[non_exhaustive]` 让未来新增变体不会破坏外部 host 的 match。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "type")]
#[non_exhaustive]
pub enum CoreEvent {
    NetworkStatusChanged {
        status: NetworkStatus,
    },
    DevicesChanged {
        devices: Vec<Device>,
    },
    PairingRequestReceived {
        #[cfg_attr(feature = "specta", specta(type = String))]
        peer_id: NodeId,
        /// core 内部生成的关联 id（新内核 RPC handler 天然长 await，不再是旧内核
        /// pending 响应 id）；UI 用 `respond_pairing_request` 回带它解决对应 handler。
        pending_id: u64,
        #[serde(flatten)]
        request: PairingRequest,
    },
    PairingCompleted {
        peer_id: String,
    },
    PairedDeviceAdded {
        device: PairedDeviceInfo,
    },
    TransferOfferReceived {
        offer: TransferOfferEvent,
    },
    TransferProgress {
        event: TransferProgressEvent,
    },
    TransferAccepted {
        event: TransferAcceptedEvent,
    },
    TransferRejected {
        event: TransferRejectedEvent,
    },
    TransferCompleted {
        event: TransferCompleteEvent,
    },
    TransferFailed {
        event: TransferFailedEvent,
    },
    TransferPaused {
        event: TransferPausedEvent,
    },
    TransferResumed {
        event: TransferResumedEvent,
    },
    TransferDbError {
        event: TransferDbErrorEvent,
    },
    /// 传输投影更新（redesign：前端唯一状态源，逐步替代分散的 Transfer* 事件）。
    TransferProjection {
        projection: TransferProjection,
    },
    PrepareProgress {
        event: PrepareProgressEvent,
    },
    Error {
        message: String,
    },
}

/// 宿主事件分发。
#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: CoreEvent) -> AppResult<()>;
}

/// 内存 host 适配器集合，供 core 单元测试和早期集成验证使用。
#[derive(Debug, Clone)]
pub struct MemoryHost {
    inner: Arc<Mutex<MemoryHostInner>>,
    paths: CoreAppPaths,
}

#[derive(Debug, Default)]
struct MemoryHostInner {
    identity: Option<DeviceIdentityBytes>,
    webrtc_certificate_pem: Option<String>,
    migration_state: Option<IdentityMigrationState>,
    paired_devices: Vec<PairedDeviceInfo>,
    events: Vec<CoreEvent>,
    sources: HashMap<FileSourceId, (HostFileMetadata, Vec<u8>)>,
    sinks: HashMap<FileSinkId, Vec<u8>>,
    notifications: Vec<Notification>,
    updates: Vec<UpdateInstallRequest>,
}

impl MemoryHost {
    pub fn new(paths: CoreAppPaths) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MemoryHostInner::default())),
            paths,
        }
    }

    pub fn with_source(
        self,
        source: FileSourceId,
        metadata: HostFileMetadata,
        data: Vec<u8>,
    ) -> Self {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sources
            .insert(source, (metadata, data));
        self
    }

    pub fn events(&self) -> Vec<CoreEvent> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .events
            .clone()
    }

    pub fn sink_bytes(&self, sink: &FileSinkId) -> Option<Vec<u8>> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sinks
            .get(sink)
            .cloned()
    }
}

#[async_trait]
impl KeychainProvider for MemoryHost {
    async fn load_identity(&self) -> AppResult<Option<DeviceIdentityBytes>> {
        Ok(self
            .inner
            .lock()
            .expect("memory host poisoned")
            .identity
            .clone())
    }

    async fn save_identity(&self, identity: DeviceIdentityBytes) -> AppResult<()> {
        self.inner.lock().expect("memory host poisoned").identity = Some(identity);
        Ok(())
    }

    async fn delete_identity(&self) -> AppResult<()> {
        self.inner.lock().expect("memory host poisoned").identity = None;
        Ok(())
    }

    async fn load_webrtc_certificate_pem(&self) -> AppResult<Option<String>> {
        Ok(self
            .inner
            .lock()
            .expect("memory host poisoned")
            .webrtc_certificate_pem
            .clone())
    }

    async fn save_webrtc_certificate_pem(&self, pem: String) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .webrtc_certificate_pem = Some(pem);
        Ok(())
    }

    async fn load_migration_state(&self) -> AppResult<IdentityMigrationState> {
        Ok(self
            .inner
            .lock()
            .expect("memory host poisoned")
            .migration_state
            .unwrap_or(IdentityMigrationState::NotStarted))
    }

    async fn save_migration_state(&self, state: IdentityMigrationState) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .migration_state = Some(state);
        Ok(())
    }

    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>> {
        Ok(self
            .inner
            .lock()
            .expect("memory host poisoned")
            .paired_devices
            .clone())
    }

    async fn save_paired_devices(&self, devices: Vec<PairedDeviceInfo>) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .paired_devices = devices;
        Ok(())
    }
}

#[async_trait]
impl EventBus for MemoryHost {
    async fn publish(&self, event: CoreEvent) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .events
            .push(event);
        Ok(())
    }
}

impl AppPaths for MemoryHost {
    fn paths(&self) -> AppResult<CoreAppPaths> {
        Ok(self.paths.clone())
    }
}

#[async_trait]
impl FileAccess for MemoryHost {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sources
            .get(source)
            .map(|(metadata, _)| metadata.clone())
            .ok_or_else(|| {
                crate::AppError::Transfer(format!("file source not found: {}", source.0))
            })
    }

    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>> {
        let inner = self.inner.lock().expect("memory host poisoned");
        let (_, data) = inner.sources.get(source).ok_or_else(|| {
            crate::AppError::Transfer(format!("file source not found: {}", source.0))
        })?;
        let start = usize::try_from(offset)
            .map_err(|_| crate::AppError::Transfer("offset is too large".to_string()))?;
        let end = start.saturating_add(length).min(data.len());
        Ok(data.get(start..end).unwrap_or_default().to_vec())
    }

    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        let sink = FileSinkId(metadata.relative_path);
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sinks
            .insert(sink.clone(), Vec::new());
        Ok(sink)
    }

    /// 续传/恢复语义：已存在的 sink 保留其字节（对应真实 host 打开既有 `.part`），
    /// 不存在才新建空 buffer。默认实现会调 `create_sink` 清空，会破坏断点续传保真度。
    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        let sink = FileSinkId(metadata.relative_path);
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sinks
            .entry(sink.clone())
            .or_default();
        Ok(sink)
    }

    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()> {
        let mut inner = self.inner.lock().expect("memory host poisoned");
        let buf = inner
            .sinks
            .get_mut(sink)
            .ok_or_else(|| crate::AppError::Transfer(format!("file sink not found: {}", sink.0)))?;
        let start = usize::try_from(offset)
            .map_err(|_| crate::AppError::Transfer("offset is too large".to_string()))?;
        if buf.len() < start {
            buf.resize(start, 0);
        }
        if buf.len() < start + data.len() {
            buf.resize(start + data.len(), 0);
        }
        buf[start..start + data.len()].copy_from_slice(&data);
        Ok(())
    }

    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<FinalizedSink> {
        if self
            .inner
            .lock()
            .expect("memory host poisoned")
            .sinks
            .contains_key(sink)
        {
            // 内存 host 的 sink id 即 relative_path;父目录 = 去掉末段(平铺文件为空串)。
            let uri = sink.0.clone();
            let dir = uri
                .rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            Ok(FinalizedSink { uri, dir })
        } else {
            Err(crate::AppError::Transfer(format!(
                "file sink not found: {}",
                sink.0
            )))
        }
    }

    async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sinks
            .remove(sink);
        Ok(())
    }
}

#[async_trait]
impl Notifier for MemoryHost {
    async fn notify(&self, notification: Notification) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .notifications
            .push(notification);
        Ok(())
    }
}

#[async_trait]
impl UpdateInstaller for MemoryHost {
    async fn install_update(&self, request: UpdateInstallRequest) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .updates
            .push(request);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use swarmdrop_net::{NodeId, SecretKey};

    use super::{
        AppPaths, CoreAppPaths, CoreEvent, CoreSaveLocation, DeviceIdentityBytes, EventBus,
        FileAccess, FileSinkId, FileSourceId, HostFileMetadata, IdentityMigrationState,
        KeychainProvider, MemoryHost,
    };
    use crate::device::{OsInfo, PairedDeviceInfo};
    use crate::network::NetworkStatus;
    use crate::protocol::{PairingMethod, PairingRequest};

    fn memory_host() -> MemoryHost {
        MemoryHost::new(CoreAppPaths {
            data_dir: PathBuf::from("data"),
            cache_dir: PathBuf::from("cache"),
            temp_dir: PathBuf::from("temp"),
            log_dir: PathBuf::from("log"),
        })
    }

    fn peer_id() -> NodeId {
        SecretKey::generate().node_id()
    }

    fn os_info(hostname: &str) -> OsInfo {
        OsInfo {
            name: None,
            hostname: hostname.to_string(),
            os: "test".to_string(),
            platform: "test".to_string(),
            arch: "test".to_string(),
            capabilities: Vec::new(),
        }
    }

    #[tokio::test]
    async fn memory_host_should_round_trip_identity_and_paired_devices() {
        let host = memory_host();
        let identity = DeviceIdentityBytes {
            keypair: vec![1, 2, 3, 4],
        };

        assert_eq!(host.load_identity().await.unwrap(), None);
        host.save_identity(identity.clone()).await.unwrap();
        assert_eq!(host.load_identity().await.unwrap(), Some(identity));

        assert_eq!(
            host.load_migration_state().await.unwrap(),
            IdentityMigrationState::NotStarted
        );
        host.save_migration_state(IdentityMigrationState::Completed)
            .await
            .unwrap();
        assert_eq!(
            host.load_migration_state().await.unwrap(),
            IdentityMigrationState::Completed
        );

        let device = PairedDeviceInfo::new(peer_id(), os_info("phone"), 42);
        host.save_paired_devices(vec![device.clone()])
            .await
            .unwrap();
        assert_eq!(host.load_paired_devices().await.unwrap().len(), 1);
        assert_eq!(
            host.load_paired_devices().await.unwrap()[0].peer_id,
            device.peer_id
        );

        host.delete_identity().await.unwrap();
        assert_eq!(host.load_identity().await.unwrap(), None);
    }

    #[tokio::test]
    async fn memory_host_should_record_network_and_pairing_events() {
        let host = memory_host();
        let peer_id = peer_id();

        host.publish(CoreEvent::NetworkStatusChanged {
            status: NetworkStatus::default(),
        })
        .await
        .unwrap();
        host.publish(CoreEvent::PairingRequestReceived {
            peer_id,
            pending_id: 7,
            request: PairingRequest {
                os_info: os_info("desktop"),
                timestamp: 123,
                method: PairingMethod::Direct,
            },
        })
        .await
        .unwrap();

        let events = host.events();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], CoreEvent::NetworkStatusChanged { .. }));
        assert!(matches!(
            events[1],
            CoreEvent::PairingRequestReceived { pending_id: 7, .. }
        ));
    }

    #[tokio::test]
    async fn memory_host_file_access_should_read_write_finalize_and_cleanup() {
        let source = FileSourceId("source.txt".to_string());
        let metadata = HostFileMetadata {
            name: "source.txt".to_string(),
            relative_path: "nested/source.txt".to_string(),
            size: 11,
            modified_at: Some(100),
            checksum: None,
            save_dir: None,
        };
        let host =
            memory_host().with_source(source.clone(), metadata.clone(), b"hello world".to_vec());

        assert_eq!(host.source_metadata(&source).await.unwrap(), metadata);
        assert_eq!(
            host.read_source_chunk(&source, 6, 5).await.unwrap(),
            b"world".to_vec()
        );

        let sink = host
            .create_sink(HostFileMetadata {
                name: "out.bin".to_string(),
                relative_path: "out.bin".to_string(),
                size: 8,
                modified_at: None,
                checksum: Some("unused-in-memory-host".to_string()),
                save_dir: Some(CoreSaveLocation::Path {
                    path: "/tmp/memory-host".to_string(),
                }),
            })
            .await
            .unwrap();
        assert_eq!(sink, FileSinkId("out.bin".to_string()));

        host.write_sink_chunk(&sink, 4, b"drop".to_vec())
            .await
            .unwrap();
        host.write_sink_chunk(&sink, 0, b"swarm".to_vec())
            .await
            .unwrap();
        let finalized = host.finalize_sink(&sink).await.unwrap();
        // 内存 host:sink id 即 relative_path;平铺文件("out.bin")父目录为空串。
        assert_eq!(finalized.uri, "out.bin");
        assert_eq!(finalized.dir, "");
        assert_eq!(host.sink_bytes(&sink).unwrap(), b"swarmrop".to_vec());

        host.cleanup_sink(&sink).await.unwrap();
        assert_eq!(host.sink_bytes(&sink), None);
    }

    #[test]
    fn memory_host_should_return_configured_app_paths() {
        let host = memory_host();
        let paths = host.paths().unwrap();
        assert_eq!(paths.data_dir, PathBuf::from("data"));
        assert_eq!(paths.cache_dir, PathBuf::from("cache"));
        assert_eq!(paths.temp_dir, PathBuf::from("temp"));
        assert_eq!(paths.log_dir, PathBuf::from("log"));
    }
}
