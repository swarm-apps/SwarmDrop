//! 读取侧的本地文件系统操作。
//!
//! 所有阻塞 I/O 通过 `tokio::task::spawn_blocking` 包装为异步。
//!
//! **只放 `FileAccess` 契约要求的能力**。目录扫描（把一棵目录树摊平成给用户挑选的列表）
//! 不在契约里，它是宿主自己的界面能力，各宿主的返回形状也不同——留在各宿主侧。

use std::path::Path;

use swarmdrop_host::AppResult;

/// 本地文件或目录的基本信息。
///
/// 刻意不直接返回 `HostFileMetadata`：那个类型带着接收侧才有意义的字段
/// （`save_dir` / `checksum`），而这里只是一次 stat。转换由调用方按自己的语境做。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFileStat {
    pub name: String,
    /// 文件字节数；目录为 0。
    pub size: u64,
    pub is_dir: bool,
}

/// 精确按 `[offset, offset+length)` 读取，契约与事故背景见 [`read_at_sync`]。
pub async fn read_at(path: &Path, offset: u64, length: usize) -> AppResult<Vec<u8>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || read_at_sync(&path, offset, length)).await?
}

/// 获取文件或目录的元数据。
pub async fn metadata(path: &Path) -> AppResult<LocalFileStat> {
    let meta = tokio::fs::metadata(path).await?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    Ok(LocalFileStat {
        name,
        size: if meta.is_file() { meta.len() } else { 0 },
        is_dir: meta.is_dir(),
    })
}

/// 在指定偏移量写入数据到文件
pub async fn write_chunk(path: &Path, offset: u64, data: Vec<u8>) -> AppResult<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || write_chunk_sync(&path, offset, &data)).await?
}

// ============ 同步内部实现 ============

/// 精确按 `[offset, offset+length)` 读，越过文件尾自动截断（EOF 读返回空）。
///
/// 这是 `FileAccess::read_source_chunk` 的契约：调用方（bao outboard 构建与 sender 的
/// 逐块推送，2026-08 起两者粒度都是 `CHUNK_SIZE`）依赖返回的字节数与请求一致。**不要**
/// 退回「offset 取整到 chunk、忽略 length」的旧语义——那会让 blake3 subtree 断言在
/// prepare 阶段直接 panic（2026-07 图片传输事故，当时 outboard 按 16KiB 粒度读，
/// >16KiB 的文件必炸而 ≤16KiB 的恰好读对，于是症状是「小文本正常、图片必炸」）。
fn read_at_sync(path: &Path, offset: u64, length: usize) -> AppResult<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    // seek 越过 EOF 合法，后续 read 自然返回 0 → 越界读得空、尾部自动截断。
    // 不预查文件大小：省一次 fstat，也消掉 stat 与 read 之间文件变短的竞态窗口。
    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; length];
    let mut filled = 0;
    while filled < length {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
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

    /// 传输层的分块粒度。这里只是测试用的一个「整块」尺寸——本模块的契约与任何块尺寸
    /// **无关**（不对齐、不取整），所以刻意不从传输层引入这个常量。
    const CHUNK_SIZE: usize = 256 * 1024;

    #[tokio::test]
    async fn test_read_at_exact_offset_and_length() {
        let dir = std::env::temp_dir().join("swarmdrop_test_read_at");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test.bin");

        // 98061 字节 = 2026-07 图片传输事故的真实大小（>16KiB 非对齐）
        let data: Vec<u8> = (0..98061).map(|i| (i % 256) as u8).collect();
        std::fs::write(&file_path, &data).unwrap();

        // 任意非对齐 offset + 任意 length 都必须精确返回请求区间。16384 这个数取自
        // 事故当时 bao 的 leaf 粒度；chunk group 改成 CHUNK_SIZE 之后它不再是真实调用
        // 粒度，但**对齐从来不是契约的一部分**，所以这条断言照测不误。
        let leaf1 = read_at(&file_path, 16384, 16384).await.unwrap();
        assert_eq!(leaf1, &data[16384..32768], "非对齐 offset 必须精确定位");

        // 尾部截断：请求越过 EOF 只还文件内的部分
        let tail = read_at(&file_path, 98061 - 100, 16384).await.unwrap();
        assert_eq!(tail, &data[98061 - 100..]);

        // 整 CHUNK_SIZE 读（sender 路径）
        let all = read_at(&file_path, 0, CHUNK_SIZE).await.unwrap();
        assert_eq!(all, data);

        // EOF 之后读 → 空（AsyncSliceReader 语义，不报错）
        assert!(read_at(&file_path, 98061, 1024).await.unwrap().is_empty());
        assert!(read_at(&file_path, 200_000, 1024).await.unwrap().is_empty());

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_read_at_empty_file() {
        let dir = std::env::temp_dir().join("swarmdrop_test_read_empty");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("empty.bin");
        std::fs::File::create(&file_path).unwrap();

        let chunk = read_at(&file_path, 0, 1024).await.unwrap();
        assert!(chunk.is_empty());

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
