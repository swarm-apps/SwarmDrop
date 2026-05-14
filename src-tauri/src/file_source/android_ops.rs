//! Android 文件操作
//!
//! 通过 `tauri-plugin-android-fs` 操作 Android SAF/MediaStore 文件。
//! 仅在 Android 平台编译（由父模块 `#[cfg(target_os = "android")]` 控制）。
//!
//! 轻量 JNI 调用（metadata、read_dir）使用 async API 直接 await；
//! 重 I/O（文件读取、哈希计算）先 async 获取文件句柄，再 spawn_blocking 执行。

use tauri_plugin_android_fs::{AndroidFsExt, Entry, FileUri};

use crate::file_source::{EnumeratedFile, FileSource, FileSourceMetadata, CHUNK_SIZE};
use crate::{AppError, AppResult};

/// 读取文件的指定分块
///
/// async API 获取 `std::fs::File` 句柄，`spawn_blocking` 中执行 seek + read。
pub async fn read_chunk(
    file_uri: &FileUri,
    file_size: u64,
    chunk_index: u32,
    app: &tauri::AppHandle,
) -> AppResult<Vec<u8>> {
    if file_size == 0 {
        return Ok(Vec::new());
    }

    let offset = chunk_index as u64 * CHUNK_SIZE as u64;
    if offset >= file_size {
        return Err(AppError::Transfer(format!(
            "chunk_index 超出范围: offset={offset}, file_size={file_size}"
        )));
    }

    let remaining = file_size - offset;
    let read_size = (remaining as usize).min(CHUNK_SIZE);

    let mut file = app
        .android_fs_async()
        .open_file_readable(file_uri)
        .await
        .map_err(|e| AppError::Transfer(format!("Android 打开文件失败: {e}")))?;

    tokio::task::spawn_blocking(move || {
        use std::io::{Read, Seek, SeekFrom};

        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; read_size];
        file.read_exact(&mut buf)?;
        Ok(buf)
    })
    .await?
}

/// 流式计算 BLAKE3 hash（hex 编码）
///
/// async API 获取文件句柄，`spawn_blocking` 中流式哈希。
pub async fn compute_hash(file_uri: &FileUri, app: &tauri::AppHandle) -> AppResult<String> {
    let mut file = app
        .android_fs_async()
        .open_file_readable(file_uri)
        .await
        .map_err(|e| AppError::Transfer(format!("Android 打开文件失败: {e}")))?;

    tokio::task::spawn_blocking(move || {
        let mut hasher = blake3::Hasher::new();
        hasher.update_reader(&mut file)?;
        Ok(hasher.finalize().to_hex().to_string())
    })
    .await?
}

/// 流式计算 BLAKE3 hash，每读取一个 chunk 调用 `on_progress(已读字节数)`
pub async fn compute_hash_with_progress(
    file_uri: &FileUri,
    app: &tauri::AppHandle,
    on_progress: impl Fn(u64) + Send + 'static,
) -> AppResult<String> {
    let mut file = app
        .android_fs_async()
        .open_file_readable(file_uri)
        .await
        .map_err(|e| AppError::Transfer(format!("Android 打开文件失败: {e}")))?;

    tokio::task::spawn_blocking(move || {
        use std::io::Read;

        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut total_read: u64 = 0;

        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            total_read += n as u64;
            on_progress(total_read);
        }

        Ok(hasher.finalize().to_hex().to_string())
    })
    .await?
}

/// 获取文件或目录的元数据
///
/// 轻量 JNI 调用，直接使用 async API。
pub async fn metadata(file_uri: &FileUri, app: &tauri::AppHandle) -> AppResult<FileSourceMetadata> {
    let entry = app
        .android_fs_async()
        .get_info(file_uri)
        .await
        .map_err(|e| AppError::Transfer(format!("Android 获取元数据失败: {e}")))?;

    match entry {
        Entry::File { name, len, .. } => Ok(FileSourceMetadata {
            name,
            size: len,
            is_dir: false,
        }),
        Entry::Dir { name, .. } => Ok(FileSourceMetadata {
            name,
            size: 0,
            is_dir: true,
        }),
    }
}

/// 递归遍历目录，返回所有文件的扁平化列表
///
/// 使用栈式迭代避免 async 递归。每层 `read_dir` 是轻量 JNI 调用，直接 await。
pub async fn enumerate_dir(
    file_uri: &FileUri,
    parent_relative_path: &str,
    app: &tauri::AppHandle,
) -> AppResult<Vec<EnumeratedFile>> {
    let mut files = Vec::new();
    let mut stack: Vec<(FileUri, String)> =
        vec![(file_uri.clone(), parent_relative_path.to_owned())];

    while let Some((uri, parent_path)) = stack.pop() {
        let entries: Vec<Entry> = app
            .android_fs_async()
            .read_dir(&uri)
            .await
            .map_err(|e| AppError::Transfer(format!("Android 读取目录失败: {e}")))?
            .collect();

        for entry in entries {
            match entry {
                Entry::File { uri, name, len, .. } => {
                    let relative_path = if parent_path.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{}", parent_path, name)
                    };

                    files.push(EnumeratedFile {
                        name,
                        relative_path,
                        source: FileSource::AndroidUri(uri),
                        size: len,
                    });
                }
                Entry::Dir { uri, name, .. } => {
                    let sub_path = if parent_path.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{}", parent_path, name)
                    };
                    stack.push((uri, sub_path));
                }
            }
        }
    }

    Ok(files)
}
