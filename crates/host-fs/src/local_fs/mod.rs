//! [`FileAccess`] 的本地文件系统实现。
//!
//! 适用于**以本地路径为落点**的宿主（桌面、命令行）。落点语义不同的宿主不在此列：
//! 移动端走系统文档提供方（Android SAF / iOS），Web 走浏览器存储，各自实现。
//!
//! ## 为什么直接实现 trait，而不保留一层来源分派
//!
//! 桌面此前是三层：trait 实现 → `FileSource`/`FileSink` 的 enum 分派 → 路径操作纯函数。
//! 中间那层**两个 enum 各自只剩一个变体**（`Path`），是历史上存在多种来源类型时留下的。
//! 迁移时跨过它：[`FileSourceId`] 本质是文件路径字符串，保存位置从
//! `HostFileMetadata.save_dir` 取，不经过 enum 也不丢任何信息。
//!
//! 目录扫描（把一棵目录树摊平给用户挑选）**不属于 `FileAccess` 契约**，留在各宿主侧——
//! 它的返回形状是宿主界面的事（桌面那份还要跨 IPC 带类型给前端）。

pub mod part_file;
pub mod sink_ops;
pub mod source_ops;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use swarmdrop_host::{
    AppError, AppResult, CoreSaveLocation, FileAccess, FileSinkId, FileSourceId, FinalizedSink,
    HostFileMetadata,
};

use part_file::PartFile;

/// 本地文件系统的 [`FileAccess`] 实现。
///
/// 持有进行中的暂存表：`write_sink_chunk` 要按 [`FileSinkId`] 找回那条 `.part` 的句柄。
#[derive(Debug, Default)]
pub struct LocalFileAccess {
    active_sinks: DashMap<FileSinkId, Arc<PartFile>>,
}

impl LocalFileAccess {
    pub fn new() -> Self {
        Self::default()
    }
}

/// [`FileSourceId`] → 本地路径。
///
/// 兼容历史上以 JSON 编码的来源描述（`{"type":"path","path":"…"}`）：解析得出就取其
/// `path`，否则把整个 id 当作路径。两种形态都出现过，而 id 是运行期生成的、不落库，
/// 所以这里只需宽容读，不需要版本协商。
fn source_path(id: &FileSourceId) -> PathBuf {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&id.0)
        && let Some(path) = value.get("path").and_then(|p| p.as_str())
    {
        return PathBuf::from(path);
    }
    PathBuf::from(&id.0)
}

/// 从接收元数据取用户选定的保存目录。
///
/// **缺失即协议错误**：保存目录由上层在接受传输时填入。此前有过「缺失就回落到一个全局
/// 共享目录」的旧路径，那会让多个会话无法各自选目录，已废弃——回落比报错更难查。
fn save_dir(metadata: &HostFileMetadata) -> AppResult<PathBuf> {
    match metadata.save_dir.as_ref() {
        Some(CoreSaveLocation::Path { path }) => Ok(PathBuf::from(path)),
        None => Err(AppError::Transfer(
            "HostFileMetadata.save_dir 缺失：上层未注入用户选择的保存目录".into(),
        )),
    }
}

#[async_trait]
impl FileAccess for LocalFileAccess {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata> {
        let stat = source_ops::metadata(&source_path(source)).await?;
        Ok(HostFileMetadata {
            name: stat.name.clone(),
            relative_path: stat.name,
            size: stat.size,
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
    ) -> AppResult<Vec<u8>> {
        source_ops::read_at(&source_path(source), offset, length).await
    }

    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        let dir = save_dir(&metadata)?;
        let part_file =
            sink_ops::create_part_file(&dir, &metadata.relative_path, metadata.size).await?;
        let sink_id = FileSinkId(metadata.relative_path);
        self.active_sinks
            .insert(sink_id.clone(), Arc::new(part_file));
        Ok(sink_id)
    }

    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        let dir = save_dir(&metadata)?;
        let part_file =
            sink_ops::open_or_create_part_file(&dir, &metadata.relative_path, metadata.size)
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
    ) -> AppResult<()> {
        let part_file = self
            .active_sinks
            .get(sink)
            .ok_or_else(|| AppError::Transfer(format!("file sink not found: {}", sink.0)))?
            .clone();
        part_file.write_at(offset, &data).await
    }

    /// 发布：`.part` → 最终路径（同盘 rename，原子）。
    ///
    /// 失败时 `.part` **仍在盘上**——续传经 `open_or_create_sink` 重新接上它（大小匹配即
    /// 复用），不需要重传任何数据块。
    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<FinalizedSink> {
        let (_, part_file) = self
            .active_sinks
            .remove(sink)
            .ok_or_else(|| AppError::Transfer(format!("file sink not found: {}", sink.0)))?;

        let path = match part_file.publish().await {
            Ok(path) => path,
            Err(err) => {
                // **失败要放回去。** 发布失败不代表这条 sink 作废——`.part` 还在盘上，
                // 取消路径的 `cleanup_sink` 要靠表里这一条才找得到它去删。摘走不放回，
                // 用户取消之后一个全尺寸的 `.part` 会永久留在他的保存目录里。
                self.active_sinks.insert(sink.clone(), part_file);
                return Err(err);
            }
        };

        // 父目录 = 落盘绝对路径的 dirname（本地文件系统语义，直接可打开）。
        let dir = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(FinalizedSink {
            uri: path.to_string_lossy().into_owned(),
            dir,
        })
    }

    async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        if let Some((_, part_file)) = self.active_sinks.remove(sink) {
            part_file.cleanup().await;
        }
        Ok(())
    }

    /// `uri` 就是 [`finalize_sink`](Self::finalize_sink) 返回的**绝对路径**，直接删。
    ///
    /// 文件已不存在按契约返回 `Ok`：删除幂等，而重试路径上「删两次」很常见。
    async fn delete_finalized_file(&self, uri: &str) -> AppResult<()> {
        match tokio::fs::remove_file(uri).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 历史 JSON 形态与纯路径形态都要解得出同一个路径。
    #[test]
    fn source_id_accepts_both_shapes() {
        let plain = FileSourceId("/tmp/a.txt".into());
        assert_eq!(source_path(&plain), PathBuf::from("/tmp/a.txt"));

        let json = FileSourceId(r#"{"type":"path","path":"/tmp/b.txt"}"#.into());
        assert_eq!(source_path(&json), PathBuf::from("/tmp/b.txt"));
    }

    /// 保存目录缺失必须报错，不得回落到任何默认目录。
    #[test]
    fn missing_save_dir_is_an_error() {
        let metadata = HostFileMetadata {
            name: "a.txt".into(),
            relative_path: "a.txt".into(),
            size: 1,
            modified_at: None,
            checksum: None,
            save_dir: None,
        };
        assert!(save_dir(&metadata).is_err());
    }
}
