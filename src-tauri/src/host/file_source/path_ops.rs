//! 标准文件系统路径操作
//!
//! 所有阻塞 I/O 操作通过 `tokio::task::spawn_blocking` 包装为异步。

use std::path::Path;

use crate::host::file_source::{EnumeratedFile, FileSource, FileSourceMetadata, CHUNK_SIZE};
use swarmdrop_core::{AppError, AppResult};

// ============ FileSource 分派方法 ============

/// 读取文件的指定分块
pub async fn read_chunk(path: &Path, file_size: u64, chunk_index: u32) -> AppResult<Vec<u8>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || read_chunk_sync(&path, file_size, chunk_index)).await?
}

/// 流式计算 BLAKE3 hash（hex 编码）
pub async fn compute_hash(path: &Path) -> AppResult<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || compute_hash_sync(&path)).await?
}

/// 流式计算 BLAKE3 hash，每读取一个 chunk 调用 `on_progress(已读字节数)`
pub async fn compute_hash_with_progress(
    path: &Path,
    on_progress: impl Fn(u64) + Send + 'static,
) -> AppResult<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || compute_hash_sync_with_progress(&path, on_progress)).await?
}

/// 获取文件或目录的元数据
pub async fn metadata(path: &Path) -> AppResult<FileSourceMetadata> {
    let meta = tokio::fs::metadata(path).await?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    Ok(FileSourceMetadata {
        name,
        size: if meta.is_file() { meta.len() } else { 0 },
        is_dir: meta.is_dir(),
    })
}

/// 递归遍历目录，返回所有文件的扁平化列表
pub async fn enumerate_dir(
    path: &Path,
    parent_relative_path: &str,
) -> AppResult<Vec<EnumeratedFile>> {
    let path = path.to_path_buf();
    let parent = parent_relative_path.to_owned();
    tokio::task::spawn_blocking(move || enumerate_dir_sync(&path, &parent)).await?
}

// ============ 接收方使用的独立方法 ============

/// 在指定偏移量写入数据到文件
pub async fn write_chunk(path: &Path, offset: u64, data: Vec<u8>) -> AppResult<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || write_chunk_sync(&path, offset, &data)).await?
}

/// 校验文件的 BLAKE3 checksum
pub async fn verify_hash(path: &Path, expected_hex: &str) -> AppResult<bool> {
    let path = path.to_path_buf();
    let expected = expected_hex.to_owned();
    tokio::task::spawn_blocking(move || {
        let actual = compute_hash_sync(&path)?;
        Ok(actual == expected)
    })
    .await?
}

// ============ 同步内部实现 ============

fn read_chunk_sync(path: &Path, file_size: u64, chunk_index: u32) -> AppResult<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    // 空文件：返回空数据
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

    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; read_size];
    file.read_exact(&mut buf)?;

    Ok(buf)
}

fn compute_hash_sync(path: &Path) -> AppResult<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(&mut file)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn compute_hash_sync_with_progress(path: &Path, on_progress: impl Fn(u64)) -> AppResult<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
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
}

fn enumerate_dir_sync(path: &Path, parent_relative_path: &str) -> AppResult<Vec<EnumeratedFile>> {
    use path_slash::PathExt as _;
    use walkdir::WalkDir;

    let mut files = Vec::new();

    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            continue;
        }

        let entry_path = entry.path();
        let name = entry_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let sub_path =
            pathdiff::diff_paths(entry_path, path).unwrap_or_else(|| entry_path.to_path_buf());
        let relative_path = if parent_relative_path.is_empty() {
            sub_path.to_slash_lossy().into_owned()
        } else {
            format!("{}/{}", parent_relative_path, sub_path.to_slash_lossy())
        };

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        files.push(EnumeratedFile {
            name,
            relative_path,
            source: FileSource::Path {
                path: entry_path.to_path_buf(),
            },
            size,
        });
    }

    Ok(files)
}

fn write_chunk_sync(path: &Path, offset: u64, data: &[u8]) -> AppResult<()> {
    use std::io::{Seek, SeekFrom, Write};

    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_chunk_basic() {
        let dir = std::env::temp_dir().join("swarmdrop_test_read_chunk");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test.bin");

        // 写入 512KB 数据（2 个 chunk）
        let data: Vec<u8> = (0..CHUNK_SIZE * 2).map(|i| (i % 256) as u8).collect();
        std::fs::write(&file_path, &data).unwrap();
        let file_size = data.len() as u64;

        // 读取第一个 chunk
        let chunk0 = read_chunk(&file_path, file_size, 0).await.unwrap();
        assert_eq!(chunk0.len(), CHUNK_SIZE);
        assert_eq!(chunk0, &data[..CHUNK_SIZE]);

        // 读取第二个 chunk
        let chunk1 = read_chunk(&file_path, file_size, 1).await.unwrap();
        assert_eq!(chunk1.len(), CHUNK_SIZE);
        assert_eq!(chunk1, &data[CHUNK_SIZE..]);

        // 越界
        assert!(read_chunk(&file_path, file_size, 2).await.is_err());

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_read_chunk_empty_file() {
        let dir = std::env::temp_dir().join("swarmdrop_test_read_empty");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("empty.bin");
        std::fs::File::create(&file_path).unwrap();

        let chunk = read_chunk(&file_path, 0, 0).await.unwrap();
        assert!(chunk.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_compute_and_verify_hash() {
        let dir = std::env::temp_dir().join("swarmdrop_test_hash");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("hash_test.bin");
        std::fs::write(&file_path, b"hello swarmdrop").unwrap();

        let hash = compute_hash(&file_path).await.unwrap();
        assert!(!hash.is_empty());

        // verify_hash 应该匹配
        assert!(verify_hash(&file_path, &hash).await.unwrap());
        // 错误的 hash 不匹配
        assert!(!verify_hash(&file_path, "0000000000000000").await.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_metadata_file() {
        let dir = std::env::temp_dir().join("swarmdrop_test_meta");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("meta_test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let meta = metadata(&file_path).await.unwrap();
        assert_eq!(meta.name, "meta_test.txt");
        assert_eq!(meta.size, 12);
        assert!(!meta.is_dir);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_metadata_dir() {
        let dir = std::env::temp_dir().join("swarmdrop_test_meta_dir");
        let _ = std::fs::create_dir_all(&dir);

        let meta = metadata(&dir).await.unwrap();
        assert_eq!(meta.name, "swarmdrop_test_meta_dir");
        assert_eq!(meta.size, 0);
        assert!(meta.is_dir);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_enumerate_dir() {
        let dir = std::env::temp_dir().join("swarmdrop_test_enum");
        let sub = dir.join("subdir");
        let _ = std::fs::create_dir_all(&sub);
        std::fs::write(dir.join("a.txt"), "aaa").unwrap();
        std::fs::write(sub.join("b.txt"), "bbb").unwrap();

        let files = enumerate_dir(&dir, "root").await.unwrap();
        assert_eq!(files.len(), 2);

        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));

        // 检查相对路径包含前缀
        for f in &files {
            assert!(f.relative_path.starts_with("root/"));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_write_chunk() {
        let dir = std::env::temp_dir().join("swarmdrop_test_write");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("write_test.bin");

        // 创建一个 512 字节的文件
        {
            let f = std::fs::File::create(&file_path).unwrap();
            f.set_len(512).unwrap();
        }

        // 在 offset 256 写入数据
        let data = vec![0xABu8; 64];
        write_chunk(&file_path, 256, data.clone()).await.unwrap();

        // 验证
        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(&content[256..320], &data[..]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
