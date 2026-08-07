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
    /// 语义 = `FileAccess::read_source_chunk` 契约；实现与契约单测见 `path_ops::read_at_sync`。
    pub async fn read_at(
        &self,
        offset: u64,
        length: usize,
        _app: &tauri::AppHandle,
    ) -> AppResult<Vec<u8>> {
        match self {
            Self::Path { path } => path_ops::read_at(path, offset, length).await,
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
    /// 已创建、尚未发布的 `.part` 写入句柄。
    active_sinks: Arc<DashMap<FileSinkId, Arc<PartFile>>>,
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
        length: usize,
    ) -> swarmdrop_core::AppResult<Vec<u8>> {
        let file_source = source_from_id(source)?;
        file_source.read_at(offset, length, &self.app).await
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
        self.active_sinks
            .insert(sink_id.clone(), Arc::new(part_file));
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
        self.active_sinks
            .insert(sink_id.clone(), Arc::new(part_file));
        Ok(sink_id)
    }

    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> swarmdrop_core::AppResult<()> {
        let part_file = self
            .active_sinks
            .get(sink)
            .ok_or_else(|| {
                swarmdrop_core::AppError::Transfer(format!("file sink not found: {}", sink.0))
            })?
            .clone();
        part_file.write_at(offset, &data).await
    }

    /// 发布：`.part` → 最终路径（同盘 rename，原子）。
    ///
    /// 失败时 sink 已从 `active_sinks` 摘除、但 `.part` **仍在盘上**——续传会经
    /// `open_or_create_sink` 重新接上它（大小匹配即复用），不需要重传任何数据块。
    async fn finalize_sink(&self, sink: &FileSinkId) -> swarmdrop_core::AppResult<FinalizedSink> {
        let (_, part_file) = self.active_sinks.remove(sink).ok_or_else(|| {
            swarmdrop_core::AppError::Transfer(format!("file sink not found: {}", sink.0))
        })?;
        let path = match part_file.publish().await {
            Ok(path) => path,
            Err(e) => {
                // **失败要放回去。** 发布失败不代表这条 sink 作废——`.part` 还在盘上，
                // 取消路径的 `cleanup_sink` 要靠表里这一条才找得到它去删。摘走不放回，
                // 用户取消之后一个全尺寸的 `.part` 会永久留在他的保存目录里。
                self.active_sinks.insert(sink.clone(), part_file);
                return Err(e);
            }
        };
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
        if let Some((_, part_file)) = self.active_sinks.remove(sink) {
            part_file.cleanup().await;
        }
        Ok(())
    }

    /// 桌面的 `uri` 就是 [`finalize_sink`](Self::finalize_sink) 返回的**绝对路径**
    /// （`.part` 重命名之后的那个），直接 `remove_file`。
    ///
    /// 文件已不存在按契约返回 `Ok`：删除幂等，而重试路径上「删两次」很常见。
    /// 此前这段逻辑在 `commands/inbox.rs` 里裸写 `tokio::fs::remove_file` —— 那是绕过端口
    /// 的第三份删除实现，现已收编到这里（编排在 `swarmdrop_transfer::inbox::delete_inbox_item`）。
    async fn delete_finalized_file(&self, uri: &str) -> swarmdrop_core::AppResult<()> {
        match tokio::fs::remove_file(uri).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
