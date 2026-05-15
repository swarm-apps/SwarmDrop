//! 文件访问 bridge —— RN 通过 expo-file-system 实现 ForeignFileAccess，
//! Rust 侧把它桥接成 core 的 [`FileAccess`] trait。
//!
//! ## 数据流
//!
//! ```text
//! core::TransferManager
//!   ├─ read_source_chunk(source_id, offset, length) ─► RN expo-fs.read()
//!   ├─ create_sink(metadata) ─────────────────────────► RN expo-fs.create()
//!   └─ write_sink_chunk(sink_id, offset, data) ───────► RN expo-fs.write()
//! ```
//!
//! ## 关键约束
//!
//! - 所有方法 async；callback 不能在 Rust 持锁时调用（uniffi-bindgen-rn 会死锁）
//! - `MobileFileMetadata` / `MobileFileSinkId` 用 uniffi Record 实现类型安全

use std::sync::Arc;

use async_trait::async_trait;
use swarmdrop_core::host::{FileAccess, FileSinkId, FileSourceId, HostFileMetadata};
use swarmdrop_core::{AppError, AppResult};

use crate::error::FfiError;

/// 文件元信息（uniffi 镜像 [`HostFileMetadata`]）
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileFileMetadata {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub modified_at: Option<i64>,
    pub checksum: Option<String>,
}

impl From<HostFileMetadata> for MobileFileMetadata {
    fn from(m: HostFileMetadata) -> Self {
        Self {
            name: m.name,
            relative_path: m.relative_path,
            size: m.size,
            modified_at: m.modified_at,
            checksum: m.checksum,
        }
    }
}

impl From<MobileFileMetadata> for HostFileMetadata {
    fn from(m: MobileFileMetadata) -> Self {
        Self {
            name: m.name,
            relative_path: m.relative_path,
            size: m.size,
            modified_at: m.modified_at,
            checksum: m.checksum,
        }
    }
}

/// RN 端必须实现的文件 I/O 接口
///
/// `source_id` 和 `sink_id` 都是字符串（host 自定义编码：桌面用 path，
/// RN 用 expo-file-system 的 uri）。core 不解析这些 id，只透传。
#[uniffi::export(with_foreign)]
#[async_trait]
pub trait ForeignFileAccess: Send + Sync {
    /// 读取 source 元信息（文件名/大小）
    async fn source_metadata(&self, source_id: String) -> Result<MobileFileMetadata, FfiError>;

    /// 读取指定 chunk —— core 用于 BLAKE3 hash 计算和 chunk 发送
    async fn read_source_chunk(
        &self,
        source_id: String,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, FfiError>;

    /// 创建写入目标，返回 sink_id
    async fn create_sink(&self, metadata: MobileFileMetadata) -> Result<String, FfiError>;

    /// 打开已有 sink 或创建新 sink（断点续传用）
    async fn open_or_create_sink(
        &self,
        metadata: MobileFileMetadata,
    ) -> Result<String, FfiError>;

    /// 写入指定偏移
    async fn write_sink_chunk(
        &self,
        sink_id: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<(), FfiError>;

    /// 校验完成，把 .part 文件最终化（host 自己实现 BLAKE3 校验）
    async fn finalize_sink(&self, sink_id: String) -> Result<(), FfiError>;

    /// 取消时清理临时文件
    async fn cleanup_sink(&self, sink_id: String) -> Result<(), FfiError>;
}

/// 把 [`ForeignFileAccess`] 适配为 core 的 [`FileAccess`]
pub(crate) struct MobileFileAccessAdapter {
    foreign: Arc<dyn ForeignFileAccess>,
}

impl MobileFileAccessAdapter {
    pub(crate) fn new(foreign: Arc<dyn ForeignFileAccess>) -> Self {
        Self { foreign }
    }
}

fn to_app_error(e: FfiError) -> AppError {
    e.into()
}

#[async_trait]
impl FileAccess for MobileFileAccessAdapter {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata> {
        let m = self
            .foreign
            .source_metadata(source.0.clone())
            .await
            .map_err(to_app_error)?;
        Ok(m.into())
    }

    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>> {
        self.foreign
            .read_source_chunk(source.0.clone(), offset, length as u64)
            .await
            .map_err(to_app_error)
    }

    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        let sink_id = self
            .foreign
            .create_sink(metadata.into())
            .await
            .map_err(to_app_error)?;
        Ok(FileSinkId(sink_id))
    }

    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        let sink_id = self
            .foreign
            .open_or_create_sink(metadata.into())
            .await
            .map_err(to_app_error)?;
        Ok(FileSinkId(sink_id))
    }

    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()> {
        self.foreign
            .write_sink_chunk(sink.0.clone(), offset, data)
            .await
            .map_err(to_app_error)
    }

    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        self.foreign
            .finalize_sink(sink.0.clone())
            .await
            .map_err(to_app_error)
    }

    async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        self.foreign
            .cleanup_sink(sink.0.clone())
            .await
            .map_err(to_app_error)
    }
}
