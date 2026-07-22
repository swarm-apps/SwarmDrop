//! Host 能力抽象（端口层）。
//!
//! Core / transfer 只依赖这些 trait，桌面端、React Native 和测试环境分别提供实现。
//! 事件聚合（`CoreEvent` / `EventBus`）与测试用 `MemoryHost` 留在 `swarmdrop-core`
//! ——它们引用 network / transfer 域的 DTO，下沉到本 crate 会成环。

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::device::PairedDeviceInfo;
use crate::error::AppResult;

/// 设备身份密钥材料。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentityBytes {
    pub keypair: Vec<u8>,
}

impl std::fmt::Debug for DeviceIdentityBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 绝不打印密钥材料
        f.debug_struct("DeviceIdentityBytes")
            .field("keypair", &"<redacted>")
            .finish()
    }
}

/// 身份存储迁移状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum IdentityMigrationState {
    NotStarted,
    Completed,
}

/// 宿主提供的安全身份存储。
#[async_trait]
pub trait KeychainProvider: Send + Sync {
    async fn load_identity(&self) -> AppResult<Option<DeviceIdentityBytes>>;
    async fn save_identity(&self, identity: DeviceIdentityBytes) -> AppResult<()>;
    async fn delete_identity(&self) -> AppResult<()>;

    /// WebRTC Direct 证书（完整 PEM，含私钥）。
    ///
    /// 它与设备 Ed25519 身份分开保存：前者固定分享地址中的 certhash，后者才是
    /// Noise 握手使用的长期身份。
    async fn load_webrtc_certificate_pem(&self) -> AppResult<Option<String>>;
    async fn save_webrtc_certificate_pem(&self, pem: String) -> AppResult<()>;
    async fn delete_webrtc_certificate_pem(&self) -> AppResult<()>;

    async fn load_migration_state(&self) -> AppResult<IdentityMigrationState>;
    async fn save_migration_state(&self, state: IdentityMigrationState) -> AppResult<()>;

    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>>;
    async fn save_paired_devices(&self, devices: Vec<PairedDeviceInfo>) -> AppResult<()>;
}

/// 应用路径集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CoreAppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub log_dir: PathBuf,
}

/// 宿主应用路径。
pub trait AppPaths: Send + Sync {
    fn paths(&self) -> AppResult<CoreAppPaths>;
}

/// 文件 source 标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileSourceId(pub String);

/// 文件 sink 标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileSinkId(pub String);

/// `finalize_sink` 的返回：文件最终落盘位置 + 其父目录 —— 都是 host 侧唯一诚实的
/// 事实源（保存目录 + 相对路径拼接推导不出:SAF document URI 有独立编码,重名冲突
/// 还会被改写成 "foo (1).txt"）。`dir` 供「打开文件夹」定位真实容器目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedSink {
    /// 文件最终 URI（桌面绝对路径 / 移动 file:// 或 SAF document URI）。
    pub uri: String,
    /// 文件父目录 URI（桌面父目录绝对路径 / 移动 file:// 目录或 SAF 目录 document URI）。
    pub dir: String,
}

/// 接收端保存位置（host-agnostic）。
///
/// core 内部统一用此类型，避免把 `entity::SaveLocation`（SeaORM 实体细节）
/// 暴露到公共 API 上。DB 边界用 [`From`] 与 `entity::SaveLocation` 双向转换。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CoreSaveLocation {
    /// 文件系统绝对路径（桌面）或 `Paths.document` 子路径（移动端）。
    Path { path: String },
}

impl From<CoreSaveLocation> for entity::SaveLocation {
    fn from(v: CoreSaveLocation) -> Self {
        match v {
            CoreSaveLocation::Path { path } => entity::SaveLocation::Path { path },
        }
    }
}

impl From<entity::SaveLocation> for CoreSaveLocation {
    fn from(v: entity::SaveLocation) -> Self {
        match v {
            entity::SaveLocation::Path { path } => CoreSaveLocation::Path { path },
        }
    }
}

/// 文件元信息。
///
/// `save_dir` 由 core 在 `accept_and_start_receive` 时填入用户选择的保存位置，
/// host adapter 据此决定真实写入路径——避免 host 端自己保存"当前会话目录"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HostFileMetadata {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub modified_at: Option<i64>,
    pub checksum: Option<String>,
    /// 接收端保存位置；source_metadata（发送端）固定为 None。
    #[serde(default)]
    pub save_dir: Option<CoreSaveLocation>,
}

/// 宿主文件访问能力。
#[async_trait]
pub trait FileAccess: Send + Sync {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata>;
    /// 精确读取源文件 `[offset, offset+length)` 区间的字节。
    ///
    /// **严格契约**（宿主实现必须逐条满足，违约会破坏 bao 逐块验签——
    /// 2026-07 桌面宿主把 offset 取整到 256KiB chunk，>16KiB 文件 prepare 直接
    /// panic 进 blake3）：
    /// - 返回字节数 == `min(length, 文件大小 - offset)`：不取整、不多读、不少读；
    /// - `offset` 越过 EOF → 返回空 `Vec`（不报错）；尾部不足 `length` → 截断到 EOF；
    /// - 禁止返回超过 `length` 的数据（内核视为违约、响错拒收）。
    ///
    /// 调用方包括按 16KiB 粒度、非对齐 offset 读的 bao outboard 构建——
    /// 不要假设 offset/length 与任何 chunk 尺寸对齐。参考实现（含契约单测）：
    /// 桌面 `src-tauri/src/host/file_source/path_ops.rs::read_at_sync`。
    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>>;

    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId>;
    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        self.create_sink(metadata).await
    }
    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()>;
    /// 校验并最终化 sink，返回文件的**最终落盘位置及其父目录**（桌面端为 .part
    /// 重命名后的绝对路径 + 其 dirname，移动端为 expo-file-system 的 file:// /
    /// SAF document URI + 对应目录 URI）。
    ///
    /// 返回值是 host 对「文件实际在哪」唯一诚实的事实源——保存目录 + 相对路径的
    /// 字符串拼接推导不出它（SAF URI 有独立的 document 段编码，重名冲突还会被
    /// host 改写成 "foo (1).txt"），core 必须原样落库供收件箱 / 「打开文件夹」消费。
    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<FinalizedSink>;
    async fn cleanup_sink(&self, _sink: &FileSinkId) -> AppResult<()> {
        Ok(())
    }
}

/// 语义通知：core 只表达「发生了什么」，不含任何语言的标题 / 正文散文。由 host 在展示
/// 时按当前 locale 翻译（桌面端走 rust-i18n；移动端目前传 `None` 不弹通知，未来可自行
/// 本地化）。与错误 `kind` 同构——core 保持语言中立，翻译发生在呈现边缘。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    /// 收到配对请求。`hostname` = 请求方设备名。
    PairingRequest { hostname: String },
    /// 收到文件传输请求（需用户确认）。`device_name` = 发送方设备名。
    IncomingTransfer { device_name: String },
}

/// 宿主通知能力。
///
/// 入参是语义 [`Notification`]，host 侧负责翻译成当前语言的标题 / 正文再展示。
/// `notify_if_unfocused` 用于桌面端：仅当窗口未聚焦时才推送通知。
/// 默认实现 fallback 到 `notify`，移动端无窗口聚焦概念时无需 override。
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, notification: Notification) -> AppResult<()>;

    async fn notify_if_unfocused(&self, notification: Notification) -> AppResult<()> {
        self.notify(notification).await
    }
}

/// 更新安装请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallRequest {
    pub url: String,
    pub is_force: bool,
}

/// 宿主更新/安装能力。
#[async_trait]
pub trait UpdateInstaller: Send + Sync {
    async fn install_update(&self, request: UpdateInstallRequest) -> AppResult<()>;
}
