//! 文件写入抽象模块（接收端）
//!
//! 与 `file_source`（读取来源）对称，`file_sink` 统一处理接收端文件的写入、
//! 校验和最终化。桌面端直接写入本地路径，Android 端通过 SAF/MediaStore 写入公共目录。
//!
//! ## 核心设计
//!
//! - **`PartFile`**：封装临时文件的元数据和写入句柄，提供 OOP 风格的操作方法。
//!   内部缓存 `std::fs::File` 句柄，`write_chunk` 使用 pwrite 实现并发安全写入。
//! - **`FileSink`**：负责创建 `PartFile`（工厂），权限检查等。

pub mod path_ops;

#[cfg(target_os = "android")]
pub mod android_ops;

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

#[cfg(target_os = "android")]
use tauri_plugin_android_fs::FileUri;

use crate::file_source::CHUNK_SIZE;
use crate::{AppError, AppResult};

/// 文件写入目标
///
/// 桌面端仅编译 `Path` 分支；Android 端同时支持两者。
#[derive(Clone)]
pub enum FileSink {
    /// 桌面端：直接写到本地目录
    Path { save_dir: PathBuf },

    /// Android：保存到公共目录（SAF/MediaStore）
    ///
    /// `subdir` 为 Download 目录下的子目录名（如 "SwarmDrop"）。
    #[cfg(target_os = "android")]
    AndroidPublicDir { subdir: String },
}

/// .part 临时文件
///
/// 封装临时文件的路径、元数据和写入句柄。
/// 写入操作通过 `write_chunk` 方法完成（内部使用 pwrite，支持并发写入）。
/// 不可 Clone——多处共享时使用 `Arc<PartFile>`。
pub struct PartFile {
    /// .part 临时文件路径（桌面端使用）
    pub part_path: PathBuf,
    /// 最终文件路径（桌面端使用，去掉 .part 后缀）
    pub final_path: PathBuf,
    /// 文件大小
    pub size: u64,
    /// 缓存的写入句柄（并发 pwrite 安全，无需外部加锁）
    write_handle: StdMutex<Option<Arc<std::fs::File>>>,
    /// Android 文件 URI（仅 Android 端使用）
    #[cfg(target_os = "android")]
    pub file_uri: Option<FileUri>,
}

impl PartFile {
    /// 创建桌面端 PartFile
    pub(crate) fn new_path(
        part_path: PathBuf,
        final_path: PathBuf,
        size: u64,
        write_handle: std::fs::File,
    ) -> Self {
        Self {
            part_path,
            final_path,
            size,
            write_handle: StdMutex::new(Some(Arc::new(write_handle))),
            #[cfg(target_os = "android")]
            file_uri: None,
        }
    }

    /// 创建 Android 端 PartFile
    #[cfg(target_os = "android")]
    pub(crate) fn new_android(
        final_path: PathBuf,
        size: u64,
        file_uri: FileUri,
        write_handle: std::fs::File,
    ) -> Self {
        Self {
            part_path: PathBuf::new(),
            final_path,
            size,
            write_handle: StdMutex::new(Some(Arc::new(write_handle))),
            file_uri: Some(file_uri),
        }
    }

    /// 创建不含写入句柄的 PartFile（仅用于 `build_part_file`）
    pub(crate) fn new_without_handle(part_path: PathBuf, final_path: PathBuf, size: u64) -> Self {
        Self {
            part_path,
            final_path,
            size,
            write_handle: StdMutex::new(None),
            #[cfg(target_os = "android")]
            file_uri: None,
        }
    }

    /// 转换为可序列化的 FileUri JSON Value（用于完成事件）
    ///
    /// Android 端将 `FileUri` 序列化为 `serde_json::Value`，桌面端返回 `None`。
    pub fn to_uri_value(&self) -> Option<serde_json::Value> {
        #[cfg(target_os = "android")]
        if let Some(uri) = &self.file_uri {
            return serde_json::to_value(uri).ok();
        }
        None
    }

    /// 写入分块数据（使用缓存句柄 + pwrite，并发安全）
    ///
    /// 内部通过 `spawn_blocking` + 定位写入（pwrite/seek_write）实现，
    /// 不修改文件偏移量，多个分块可安全并发写入同一文件。
    pub async fn write_chunk(&self, chunk_index: u32, data: &[u8]) -> AppResult<()> {
        let handle = {
            let guard = self.write_handle.lock().unwrap();
            guard
                .as_ref()
                .ok_or_else(|| AppError::Transfer("写入句柄已关闭".into()))?
                .clone()
        };

        let offset = chunk_index as u64 * CHUNK_SIZE as u64;
        let data = data.to_vec();

        tokio::task::spawn_blocking(move || write_all_at(&handle, &data, offset))
            .await?
            .map_err(|e: std::io::Error| AppError::Transfer(format!("写入分块失败: {e}")))
    }

    /// 关闭写入句柄
    ///
    /// 校验前调用，确保所有数据已落盘且文件句柄释放（Windows 下 rename 需要）。
    /// 幂等操作，多次调用安全。
    pub fn close_write_handle(&self) {
        let mut guard = self.write_handle.lock().unwrap();
        *guard = None;
    }

    /// 校验 BLAKE3 并最终化文件
    ///
    /// 1. 关闭写入句柄
    /// 2. 流式计算 BLAKE3 校验和
    /// 3. 校验通过：桌面端重命名 .part → 最终路径；Android 端 set_pending(false) + scan
    /// 4. 校验失败：删除临时文件
    pub async fn verify_and_finalize(
        &self,
        expected_checksum: &str,
        #[allow(unused_variables)] app: &tauri::AppHandle,
    ) -> AppResult<PathBuf> {
        self.close_write_handle();

        #[cfg(target_os = "android")]
        if self.file_uri.is_some() {
            return android_ops::verify_and_finalize(self, expected_checksum, app).await;
        }

        path_ops::verify_and_finalize(self, expected_checksum).await
    }

    /// 清理临时文件（静默忽略错误）
    ///
    /// 传输取消或失败时调用，删除未最终化的临时文件。
    pub async fn cleanup(&self, #[allow(unused_variables)] app: &tauri::AppHandle) {
        self.close_write_handle();

        #[cfg(target_os = "android")]
        if self.file_uri.is_some() {
            android_ops::cleanup_part_file(self, app).await;
            return;
        }

        let _ = tokio::fs::remove_file(&self.part_path).await;
    }
}

impl fmt::Debug for PartFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("PartFile");
        d.field("part_path", &self.part_path)
            .field("final_path", &self.final_path)
            .field("size", &self.size);
        #[cfg(target_os = "android")]
        d.field("file_uri", &self.file_uri);
        d.finish()
    }
}

/// 根据最终路径计算 .part 临时文件路径
///
/// 规则：在原扩展名后追加 `.part`，如 `readme.md` → `readme.md.part`；
/// 无扩展名时直接追加，如 `Makefile` → `Makefile.part`。
pub fn compute_part_path(final_path: &Path) -> PathBuf {
    final_path.with_extension(
        final_path
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".into()),
    )
}

// ============ 跨平台定位写入（pwrite） ============

/// Unix: 使用 pwrite (write_all_at)，原子定位写入，不修改文件偏移量
#[cfg(unix)]
fn write_all_at(file: &std::fs::File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(data, offset)
}

/// Windows: 使用 seek_write，循环确保完整写入
#[cfg(windows)]
fn write_all_at(file: &std::fs::File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut written = 0;
    while written < data.len() {
        let n = file.seek_write(&data[written..], offset + written as u64)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "seek_write returned 0 bytes",
            ));
        }
        written += n;
    }
    Ok(())
}

// ============ FileSink 工厂方法 ============

impl FileSink {
    /// 创建 .part 临时文件
    ///
    /// 返回带有缓存写入句柄的 `PartFile`，后续分块写入直接调用 `part_file.write_chunk()`。
    pub async fn create_part_file(
        &self,
        relative_path: &str,
        file_size: u64,
        #[allow(unused_variables)] app: &tauri::AppHandle,
    ) -> AppResult<PartFile> {
        match self {
            Self::Path { save_dir } => {
                path_ops::create_part_file(save_dir, relative_path, file_size).await
            }
            #[cfg(target_os = "android")]
            Self::AndroidPublicDir { subdir } => {
                android_ops::create_part_file(subdir, relative_path, file_size, app).await
            }
        }
    }

    /// 打开已有 .part 文件或创建新文件（断点续传用）
    ///
    /// 桌面端：检查 .part 文件存在且大小匹配时以读写模式打开，否则创建新文件。
    /// Android 端：暂不支持断点续传，回退到 create_part_file。
    pub async fn open_or_create_part_file(
        &self,
        relative_path: &str,
        file_size: u64,
        #[allow(unused_variables)] app: &tauri::AppHandle,
    ) -> AppResult<PartFile> {
        match self {
            Self::Path { save_dir } => {
                path_ops::open_or_create_part_file(save_dir, relative_path, file_size).await
            }
            #[cfg(target_os = "android")]
            Self::AndroidPublicDir { subdir } => {
                android_ops::create_part_file(subdir, relative_path, file_size, app).await
            }
        }
    }

    /// 构建 PartFile（不创建实际文件，不含写入句柄）
    ///
    /// 用于桌面端清理场景：已知 relative_path 但不需要创建文件。
    pub fn build_part_file(&self, relative_path: &str, size: u64) -> PartFile {
        match self {
            Self::Path { save_dir } => {
                let final_path = save_dir.join(relative_path);
                let part_path = compute_part_path(&final_path);
                PartFile::new_without_handle(part_path, final_path, size)
            }
            #[cfg(target_os = "android")]
            Self::AndroidPublicDir { .. } => {
                PartFile::new_without_handle(PathBuf::new(), PathBuf::new(), size)
            }
        }
    }

    /// 转换为 `SaveLocation` 枚举（用于完成事件和数据库持久化）
    pub fn to_save_location(&self) -> entity::SaveLocation {
        match self {
            Self::Path { save_dir } => entity::SaveLocation::Path {
                path: save_dir.to_string_lossy().into_owned(),
            },
            #[cfg(target_os = "android")]
            Self::AndroidPublicDir { subdir } => entity::SaveLocation::AndroidPublicDir {
                subdir: subdir.clone(),
            },
        }
    }

    /// 获取保存目录的显示字符串
    pub fn save_dir_display(&self) -> Cow<'_, str> {
        match self {
            Self::Path { save_dir } => save_dir.to_string_lossy(),
            #[cfg(target_os = "android")]
            Self::AndroidPublicDir { .. } => Cow::Borrowed("Download"),
        }
    }

    /// 请求写入权限
    ///
    /// 桌面端无需权限，始终返回 Ok。
    /// Android 端检查并请求 `WRITE_EXTERNAL_STORAGE` 权限（Android 9 及以下需要）。
    pub async fn ensure_permission(
        &self,
        #[allow(unused_variables)] app: &tauri::AppHandle,
    ) -> AppResult<()> {
        match self {
            Self::Path { .. } => Ok(()),
            #[cfg(target_os = "android")]
            Self::AndroidPublicDir { .. } => android_ops::ensure_permission(app).await,
        }
    }
}
