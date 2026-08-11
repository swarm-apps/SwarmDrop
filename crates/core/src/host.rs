//! Host 能力聚合。
//!
//! 端口层 trait + DTO（`FileAccess` / `KeychainProvider` / `CoreSaveLocation` …）已下沉到
//! [`swarmdrop_host`]，本模块 glob 再导出以保持 `crate::host::*` 路径不变。
//!
//! 留在 core 的是**聚合层**：[`CoreEvent`] 汇聚 network / pairing / transfer 各域事件
//! （且反向引用 transfer wire 类型），[`EventBus`] 以它为消息，[`MemoryHost`] 为测试替身。
//! 三者引用上层类型，无法随端口层下沉——否则 host ↔ transfer 成环。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use swarmdrop_net::NodeId;

pub use swarmdrop_host::*;

use swarmdrop_host::device::{Device, DeviceName, PairedDeviceInfo};
use swarmdrop_transfer::incoming::TransferOfferEvent;
use swarmdrop_transfer::progress::{
    FilePublishEvent, PrepareProgressEvent, TransferAcceptedEvent, TransferCompleteEvent,
    TransferDbErrorEvent, TransferFailedEvent, TransferPausedEvent, TransferProgressEvent,
    TransferRejectedEvent, TransferResumedEvent,
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
    /// 已配对设备被解除（唯一触发点是
    /// [`PairingManager::unpair`](crate::pairing::PairingManager::unpair)，且仅在集合
    /// 真的变了时才发）。
    ///
    /// 与 [`DevicesChanged`](Self::DevicesChanged) 刻意分开：后者携带的是含 presence /
    /// 连接态的设备视图，每秒可能刷新多次，表达不了「这台设备不再被信任了」。
    ///
    /// **host 不得在这个事件里再删一次持久化**——core 已经写过盘了，重复删虽然幂等，
    /// 却会让「持久化失败」这个错误被第二次成功掩盖。移除方向的事件只承担通知职责。
    PairedDeviceRemoved {
        #[cfg_attr(feature = "specta", specta(type = String))]
        peer_id: NodeId,
    },
    /// 本机设备名已变更（唯一触发点是
    /// [`rename_device`](crate::device_name::rename_device)）。
    ///
    /// 设备名在三端都有多处显示（设置页、设备卡片、onboarding 回显），还有非 UI 消费者
    /// （桌面 MCP server 的设备信息资源），所以由 core 广播一次、各处订阅，而不是让发起
    /// 改名的那个界面自己刷新。
    ///
    /// `display_name` 是 [`OsInfo::display_name`](crate::device::OsInfo::display_name)
    /// 的结果（`name` 空则回退 hostname），省得三端各写一遍 `name || hostname` 的回退。
    /// **例外**：节点未启动时改名（onboarding 路径）core 拿不到本机 `OsInfo`——hostname
    /// 的唯一装配点在 `runtime::start_node`——此时 `display_name` 退化为归一化后的名字
    /// 本身，清空则为空串，调用方按自己那套 hostname 展示。
    DeviceRenamed {
        name: Option<String>,
        display_name: String,
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
    /// 单个文件正在从暂存位置发布到用户可见位置。
    ///
    /// 与 [`TransferProgress`](Self::TransferProgress) 刻意分开：那条是会话级、200ms 一帧，
    /// 而发布是**逐文件**的（收齐即发布，散布在整条传输过程中），且发布期间没有新的字节
    /// 样本——把它挂进进度事件会让同一帧里的 speed/eta 变成陈旧值却无从分辨。
    FilePublish {
        event: FilePublishEvent,
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
#[derive(Debug, Clone, Default)]
pub struct MemoryHost {
    inner: Arc<Mutex<MemoryHostInner>>,
}

/// sink 上发生过的操作，按发生顺序记录。
///
/// 存在的理由是**时序断言**：「单个文件收齐即发布」这条行为无法从终态观察——传完之后
/// 无论哪种实现，两个文件都是已发布。只有序列能区分「f1 发布 → f2 开始写」与
/// 「两个都写完 → 再一起发布」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkOp {
    Write(FileSinkId),
    Finalize(FileSinkId),
}

#[derive(Debug, Default)]
struct MemoryHostInner {
    identity: Option<DeviceIdentityBytes>,
    webrtc_certificate_pem: Option<String>,
    paired_devices: Vec<PairedDeviceInfo>,
    device_name: Option<DeviceName>,
    events: Vec<CoreEvent>,
    sources: HashMap<FileSourceId, (HostFileMetadata, Vec<u8>)>,
    sinks: HashMap<FileSinkId, Vec<u8>>,
    sink_ops: Vec<SinkOp>,
    /// 命中的 sink 在 `finalize_sink` 时报错，用于验证「发布失败 = 可重试」的语义。
    fail_finalize: HashSet<FileSinkId>,
    notifications: Vec<Notification>,
    updates: Vec<UpdateInstallRequest>,
}

impl MemoryHost {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MemoryHostInner::default())),
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

    /// 让指定 sink 的 `finalize_sink` 报错，模拟「数据收齐了但落地失败」
    /// （空间不足 / 权限被撤 / 外部 fd 失效）。
    pub fn failing_finalize(self, sink: FileSinkId) -> Self {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .fail_finalize
            .insert(sink);
        self
    }

    /// 解除 [`failing_finalize`](Self::failing_finalize) 注入的故障，用于验证重试路径。
    pub fn clear_finalize_failures(&self) {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .fail_finalize
            .clear();
    }

    /// sink 操作的发生顺序，见 [`SinkOp`]。
    pub fn sink_ops(&self) -> Vec<SinkOp> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sink_ops
            .clone()
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

    async fn delete_webrtc_certificate_pem(&self) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .webrtc_certificate_pem = None;
        Ok(())
    }
}

#[async_trait]
impl PairedDeviceStore for MemoryHost {
    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>> {
        Ok(self
            .inner
            .lock()
            .expect("memory host poisoned")
            .paired_devices
            .clone())
    }

    async fn save_paired_devices(&self, devices: &[PairedDeviceInfo]) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .paired_devices = devices.to_vec();
        Ok(())
    }
}

#[async_trait]
impl DeviceConfig for MemoryHost {
    async fn load_device_name(&self) -> Option<DeviceName> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .device_name
            .clone()
    }

    async fn save_device_name(&self, name: Option<DeviceName>) -> AppResult<()> {
        self.inner.lock().expect("memory host poisoned").device_name = name;
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
        inner.sink_ops.push(SinkOp::Write(sink.clone()));
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
        {
            let mut inner = self.inner.lock().expect("memory host poisoned");
            inner.sink_ops.push(SinkOp::Finalize(sink.clone()));
            if inner.fail_finalize.contains(sink) {
                // 注入的落地失败：**不动 sink 的字节**——数据是好的，只是搬不过去，
                // 上层应当保留暂存并允许原地重试（见 receiver 的 publish_file）。
                return Err(crate::AppError::Transfer(format!(
                    "注入的发布失败: {}",
                    sink.0
                )));
            }
        }
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

    /// 内存 host 的 `uri` 就是 sink id（见 [`Self::finalize_sink`]），所以「删已落盘文件」
    /// 就是把那条 sink 的字节丢掉。不存在不报错——与端口契约一致（删除幂等）。
    async fn delete_finalized_file(&self, uri: &str) -> AppResult<()> {
        self.inner
            .lock()
            .expect("memory host poisoned")
            .sinks
            .remove(&FileSinkId(uri.to_string()));
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
    use swarmdrop_net::{NodeId, SecretKey};

    use super::{
        CoreEvent, CoreSaveLocation, DeviceIdentityBytes, EventBus, FileAccess, FileSinkId,
        FileSourceId, HostFileMetadata, KeychainProvider, MemoryHost, PairedDeviceStore,
    };
    use crate::device::{OsInfo, PairedDeviceInfo};
    use crate::network::NetworkStatus;
    use crate::protocol::{PairingMethod, PairingRequest};

    fn memory_host() -> MemoryHost {
        MemoryHost::new()
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

        let device = PairedDeviceInfo::new(peer_id(), os_info("phone"), 42);
        host.save_paired_devices(std::slice::from_ref(&device))
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
}
