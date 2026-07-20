//! 桌面端文件写入操作
//!
//! 提供 .part 临时文件的创建和校验/最终化实现。
//! 分块写入已由 `PartFile::write_chunk()` 统一处理（跨平台 pwrite）。

use std::path::{Path, PathBuf};

use crate::host::file_sink::{PartFile, compute_part_path};
use swarmdrop_core::{AppError, AppResult};

/// 创建 .part 临时文件：创建目录 → 创建文件 → 预分配大小 → 缓存写入句柄
pub(crate) async fn create_part_file(
    save_dir: &Path,
    relative_path: &str,
    file_size: u64,
) -> AppResult<PartFile> {
    let (part_path, final_path) = resolve_paths(save_dir, relative_path).await?;

    let f = create_new_part(&part_path, file_size).await?;
    let write_handle = f.into_std().await;

    Ok(PartFile::new_path(
        part_path,
        final_path,
        file_size,
        write_handle,
    ))
}

/// 打开已有 .part 文件或创建新文件（断点续传用）
///
/// 检查 .part 文件是否存在且大小匹配：
/// - 匹配：以读写模式打开（不截断），保留已有数据
/// - 不匹配或不存在：创建新文件并预分配大小
pub(crate) async fn open_or_create_part_file(
    save_dir: &Path,
    relative_path: &str,
    file_size: u64,
) -> AppResult<PartFile> {
    let (part_path, final_path) = resolve_paths(save_dir, relative_path).await?;

    // 检查 .part 文件是否存在且大小匹配
    let existing_ok = tokio::fs::metadata(&part_path)
        .await
        .map(|m| m.len() == file_size)
        .unwrap_or(false);

    let f = if existing_ok {
        // 以读写模式打开，不截断
        tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&part_path)
            .await?
    } else {
        create_new_part(&part_path, file_size).await?
    };

    let write_handle = f.into_std().await;
    Ok(PartFile::new_path(
        part_path,
        final_path,
        file_size,
        write_handle,
    ))
}

/// 解析最终路径和 .part 路径，并确保父目录存在
async fn resolve_paths(save_dir: &Path, relative_path: &str) -> AppResult<(PathBuf, PathBuf)> {
    let final_path = save_dir.join(relative_path);
    let part_path = compute_part_path(&final_path);

    if let Some(parent) = final_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    Ok((part_path, final_path))
}

/// 创建新的 .part 文件并预分配大小
async fn create_new_part(part_path: &Path, file_size: u64) -> AppResult<tokio::fs::File> {
    let f = tokio::fs::File::create(part_path).await?;
    if file_size > 0 {
        f.set_len(file_size).await?;
    }
    Ok(f)
}

/// 校验 BLAKE3 + 重命名 .part → 最终路径
///
/// 校验失败时删除 .part 文件。
/// 调用前需确保写入句柄已关闭（`PartFile::close_write_handle()`）。
pub(crate) async fn verify_and_finalize(
    part_file: &PartFile,
    expected_checksum: &str,
) -> AppResult<PathBuf> {
    let part_path = part_file.part_path.clone();
    let expected = expected_checksum.to_owned();

    let checksum_ok =
        tokio::task::spawn_blocking(move || verify_checksum_sync(&part_path, &expected)).await??;

    if !checksum_ok {
        let _ = tokio::fs::remove_file(&part_file.part_path).await;
        return Err(AppError::Transfer(format!(
            "文件校验失败: {}",
            part_file.final_path.display()
        )));
    }

    tokio::fs::rename(&part_file.part_path, &part_file.final_path).await?;
    Ok(part_file.final_path.clone())
}

// ============ 同步内部实现 ============

fn verify_checksum_sync(path: &Path, expected_hex: &str) -> AppResult<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(&mut file)?;
    let actual_hex = hasher.finalize().to_hex().to_string();
    Ok(actual_hex == expected_hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_part_file_basic() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_create");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "hello.txt", 1024).await.unwrap();
        assert!(part.part_path.exists());
        assert_eq!(part.final_path, dir.join("hello.txt"));
        assert_eq!(part.part_path, dir.join("hello.txt.part"));
        assert_eq!(std::fs::metadata(&part.part_path).unwrap().len(), 1024);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_create_part_file_nested() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_nested");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "docs/readme.md", 512).await.unwrap();
        assert!(part.part_path.exists());
        assert!(dir.join("docs").is_dir());
        assert_eq!(part.part_path, dir.join("docs").join("readme.md.part"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_create_part_file_no_extension() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_noext");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "Makefile", 256).await.unwrap();
        assert_eq!(part.part_path, dir.join("Makefile.part"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_create_part_file_empty() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_empty");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "empty.txt", 0).await.unwrap();
        assert!(part.part_path.exists());
        assert_eq!(std::fs::metadata(&part.part_path).unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_write_chunk_via_part_file() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_write");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "data.bin", 1024).await.unwrap();

        let data = vec![0xABu8; 512];
        part.write_at(0, &data).await.unwrap();

        // 关闭句柄后读取验证
        part.close_write_handle();

        let content = std::fs::read(&part.part_path).unwrap();
        assert_eq!(&content[..512], &data[..]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_write_chunk_multiple() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_write_multi");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let chunk_size = crate::host::file_source::CHUNK_SIZE;
        let file_size = chunk_size as u64 * 2;
        let part = create_part_file(&dir, "multi.bin", file_size)
            .await
            .unwrap();

        let data0 = vec![0xAAu8; chunk_size];
        let data1 = vec![0xBBu8; chunk_size];

        // 并发写入两个分块（write_at 按字节 offset 定位，非对齐写不再被取整）
        let (r0, r1) = tokio::join!(
            part.write_at(0, &data0),
            part.write_at(chunk_size as u64, &data1)
        );
        r0.unwrap();
        r1.unwrap();

        part.close_write_handle();

        let content = std::fs::read(&part.part_path).unwrap();
        assert_eq!(&content[..chunk_size], &data0[..]);
        assert_eq!(&content[chunk_size..chunk_size * 2], &data1[..]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_verify_and_finalize_success() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_verify_ok");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "test.txt", 0).await.unwrap();
        part.close_write_handle();
        std::fs::write(&part.part_path, b"hello swarmdrop").unwrap();

        // 计算正确的 hash
        let hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"hello swarmdrop");
            hasher.finalize().to_hex().to_string()
        };

        let final_path = verify_and_finalize(&part, &hash).await.unwrap();
        assert!(final_path.exists());
        assert!(!part.part_path.exists());
        assert_eq!(
            std::fs::read_to_string(&final_path).unwrap(),
            "hello swarmdrop"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_verify_and_finalize_failure() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_verify_fail");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "test.txt", 0).await.unwrap();
        part.close_write_handle();
        std::fs::write(&part.part_path, b"hello").unwrap();

        let result = verify_and_finalize(&part, "wrong_hash").await;
        assert!(result.is_err());
        assert!(!part.part_path.exists()); // .part 应被删除

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_cleanup_part_file() {
        let dir = std::env::temp_dir().join("swarmdrop_test_sink_cleanup");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let part = create_part_file(&dir, "temp.bin", 100).await.unwrap();
        assert!(part.part_path.exists());

        // 关闭句柄后删除
        part.close_write_handle();
        let _ = tokio::fs::remove_file(&part.part_path).await;
        assert!(!part.part_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
