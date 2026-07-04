//! 文件来源抽象模块
//!
//! 当前仅支持标准文件系统路径（桌面端）。移动端已迁移到 RN，
//! 通过 `expo-file-system` + uniffi callback 处理文件 I/O，不再走这里。

pub mod path_ops;

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use swarmdrop_core::host::{
    CoreSaveLocation, FileAccess, FileSinkId, FileSourceId, FinalizedSink, HostFileMetadata,
};

use crate::host::file_sink::{FileSink, PartFile};
use swarmdrop_core::AppResult;

/// 分块大小：256 KB
pub const CHUNK_SIZE: usize = 256 * 1024;

/// 文件来源：标准文件系统路径
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FileSource {
    /// 标准文件系统路径
    Path { path: PathBuf },
}

/// 文件元数据
#[derive(Debug, Clone)]
pub struct FileSourceMetadata {
    /// 文件或目录名
    pub name: String,
    /// 文件大小（字节），目录为 0
    pub size: u64,
    /// 是否为目录
    pub is_dir: bool,
}

/// 目录遍历后的扁平化文件条目
///
/// 同时用于 `scan_sources` 命令返回和 `prepare_send` 命令输入，
/// 因此同时派生 Serialize + Deserialize。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EnumeratedFile {
    /// 文件名
    pub name: String,
    /// 相对路径（Unix 风格 `/` 分隔符）
    pub relative_path: String,
    /// 文件来源
    pub source: FileSource,
    /// 文件大小
    pub size: u64,
}

impl FileSource {
    /// 读取文件的指定分块
    ///
    /// `file_size` 用于验证 chunk_index 范围和计算最后一块的读取量。
    pub async fn read_chunk(
        &self,
        file_size: u64,
        chunk_index: u32,
        _app: &tauri::AppHandle,
    ) -> AppResult<Vec<u8>> {
        match self {
            Self::Path { path } => path_ops::read_chunk(path, file_size, chunk_index).await,
        }
    }

    /// 流式计算 BLAKE3 hash（不将整个文件加载到内存）
    pub async fn compute_hash(&self, _app: &tauri::AppHandle) -> AppResult<String> {
        match self {
            Self::Path { path } => path_ops::compute_hash(path).await,
        }
    }

    /// 流式计算 BLAKE3 hash，每读取一个 chunk 调用 `on_progress(当前文件已读字节数)`
    pub async fn compute_hash_with_progress(
        &self,
        _app: &tauri::AppHandle,
        on_progress: impl Fn(u64) + Send + 'static,
    ) -> AppResult<String> {
        match self {
            Self::Path { path } => path_ops::compute_hash_with_progress(path, on_progress).await,
        }
    }

    /// 获取文件或目录的元数据
    pub async fn metadata(&self, _app: &tauri::AppHandle) -> AppResult<FileSourceMetadata> {
        match self {
            Self::Path { path } => path_ops::metadata(path).await,
        }
    }

    /// 递归遍历目录，返回所有文件的扁平化列表
    ///
    /// `parent_relative_path` 是当前目录在传输中的相对路径前缀。
    pub async fn enumerate_dir(
        &self,
        parent_relative_path: &str,
        _app: &tauri::AppHandle,
    ) -> AppResult<Vec<EnumeratedFile>> {
        match self {
            Self::Path { path } => path_ops::enumerate_dir(path, parent_relative_path).await,
        }
    }
}

#[derive(Clone)]
pub struct TauriFileAccess {
    app: tauri::AppHandle,
    active_sinks: Arc<DashMap<FileSinkId, ActiveSink>>,
}

#[derive(Clone)]
struct ActiveSink {
    part_file: Arc<PartFile>,
    checksum: Option<String>,
}

impl TauriFileAccess {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            active_sinks: Arc::new(DashMap::new()),
        }
    }
}

/// 从 [`HostFileMetadata::save_dir`] 构造桌面端 [`FileSink`]。
///
/// `save_dir` 由 core 在 `accept_and_start_receive` 时填入用户选择的目录，
/// 缺失即视为协议错误（旧路径走全局共享 sink，导致多会话间无法切换目录，
/// 已废弃）。
fn sink_from_metadata(metadata: &HostFileMetadata) -> swarmdrop_core::AppResult<FileSink> {
    match metadata.save_dir.as_ref() {
        Some(CoreSaveLocation::Path { path }) => Ok(FileSink::Path {
            save_dir: PathBuf::from(path),
        }),
        None => Err(swarmdrop_core::AppError::Transfer(
            "HostFileMetadata.save_dir 缺失：core 未注入用户选择的保存目录".into(),
        )),
    }
}

pub fn source_id(source: &FileSource) -> FileSourceId {
    match source {
        FileSource::Path { path } => FileSourceId(path.to_string_lossy().into_owned()),
    }
}

fn source_from_id(source: &FileSourceId) -> swarmdrop_core::AppResult<FileSource> {
    if let Ok(source) = serde_json::from_str::<FileSource>(&source.0) {
        return Ok(source);
    }
    Ok(FileSource::Path {
        path: PathBuf::from(&source.0),
    })
}

#[async_trait::async_trait]
impl FileAccess for TauriFileAccess {
    async fn source_metadata(
        &self,
        source: &FileSourceId,
    ) -> swarmdrop_core::AppResult<HostFileMetadata> {
        let source = source_from_id(source)?;
        let metadata = source.metadata(&self.app).await?;
        Ok(HostFileMetadata {
            name: metadata.name.clone(),
            relative_path: metadata.name,
            size: metadata.size,
            modified_at: None,
            checksum: None,
            save_dir: None,
        })
    }

    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        _length: usize,
    ) -> swarmdrop_core::AppResult<Vec<u8>> {
        let file_source = source_from_id(source)?;
        let metadata = file_source.metadata(&self.app).await?;
        let chunk_index = u32::try_from(offset / CHUNK_SIZE as u64)
            .map_err(|_| swarmdrop_core::AppError::Transfer("chunk offset is too large".into()))?;
        file_source
            .read_chunk(metadata.size, chunk_index, &self.app)
            .await
    }

    async fn create_sink(
        &self,
        metadata: HostFileMetadata,
    ) -> swarmdrop_core::AppResult<FileSinkId> {
        let sink = sink_from_metadata(&metadata)?;
        let part_file = sink
            .create_part_file(&metadata.relative_path, metadata.size, &self.app)
            .await?;
        let sink_id = FileSinkId(metadata.relative_path);
        self.active_sinks.insert(
            sink_id.clone(),
            ActiveSink {
                part_file: Arc::new(part_file),
                checksum: metadata.checksum,
            },
        );
        Ok(sink_id)
    }

    async fn open_or_create_sink(
        &self,
        metadata: HostFileMetadata,
    ) -> swarmdrop_core::AppResult<FileSinkId> {
        let sink = sink_from_metadata(&metadata)?;
        let part_file = sink
            .open_or_create_part_file(&metadata.relative_path, metadata.size, &self.app)
            .await?;
        let sink_id = FileSinkId(metadata.relative_path);
        self.active_sinks.insert(
            sink_id.clone(),
            ActiveSink {
                part_file: Arc::new(part_file),
                checksum: metadata.checksum,
            },
        );
        Ok(sink_id)
    }

    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> swarmdrop_core::AppResult<()> {
        let active = self
            .active_sinks
            .get(sink)
            .ok_or_else(|| {
                swarmdrop_core::AppError::Transfer(format!("file sink not found: {}", sink.0))
            })?
            .clone();
        let chunk_index = u32::try_from(offset / CHUNK_SIZE as u64)
            .map_err(|_| swarmdrop_core::AppError::Transfer("chunk offset is too large".into()))?;
        active.part_file.write_chunk(chunk_index, &data).await
    }

    async fn finalize_sink(&self, sink: &FileSinkId) -> swarmdrop_core::AppResult<FinalizedSink> {
        let (_, active) = self.active_sinks.remove(sink).ok_or_else(|| {
            swarmdrop_core::AppError::Transfer(format!("file sink not found: {}", sink.0))
        })?;
        let checksum = active.checksum.ok_or_else(|| {
            swarmdrop_core::AppError::Transfer(format!("file sink checksum missing: {}", sink.0))
        })?;
        let path = active
            .part_file
            .verify_and_finalize(&checksum, &self.app)
            .await?;
        // 父目录 = 落盘绝对路径的 dirname(桌面文件系统语义,直接可打开)。
        let dir = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(FinalizedSink {
            uri: path.to_string_lossy().into_owned(),
            dir,
        })
    }

    async fn cleanup_sink(&self, sink: &FileSinkId) -> swarmdrop_core::AppResult<()> {
        if let Some((_, active)) = self.active_sinks.remove(sink) {
            active.part_file.cleanup(&self.app).await;
        }
        Ok(())
    }
}

/// 计算文件的总分块数
pub fn calc_total_chunks(file_size: u64) -> u32 {
    if file_size == 0 {
        return 1; // 空文件也算一个块
    }
    file_size.div_ceil(CHUNK_SIZE as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calc_total_chunks() {
        assert_eq!(calc_total_chunks(0), 1);
        assert_eq!(calc_total_chunks(1), 1);
        assert_eq!(calc_total_chunks(CHUNK_SIZE as u64), 1);
        assert_eq!(calc_total_chunks(CHUNK_SIZE as u64 + 1), 2);
        assert_eq!(calc_total_chunks(CHUNK_SIZE as u64 * 10), 10);
    }
}
