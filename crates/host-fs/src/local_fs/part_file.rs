//! 暂存文件（`.part`）。
//!
//! 接收侧是「暂存 → 发布」两段（见 `FileAccess` 的 trait 文档）：数据块先随机写进一个
//! **本进程完全拥有**的 `.part`，收齐后再原子 rename 到最终位置。本模块是那个暂存的载体。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use swarmdrop_host::{AppError, AppResult};

/// `.part` 暂存文件。
///
/// 封装暂存路径、最终路径与写入句柄。写入走 `write_at`（内部 pwrite，多块可并发写同一
/// 文件而不互相干扰偏移量）。不可 Clone——多处共享时用 `Arc<PartFile>`。
pub struct PartFile {
    /// `.part` 暂存文件路径。
    pub part_path: PathBuf,
    /// 最终文件路径（去掉 `.part` 后缀）。
    pub final_path: PathBuf,
    /// 文件大小。
    pub size: u64,
    /// 缓存的写入句柄（并发 pwrite 安全，无需外部加锁）。
    write_handle: StdMutex<Option<Arc<std::fs::File>>>,
}

impl PartFile {
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
        }
    }

    /// 在字节偏移 `offset` 处定位写入（缓存句柄 + pwrite，并发安全）。
    ///
    /// 内部经 `spawn_blocking` 做定位写入，不修改文件偏移量，多个分块可安全并发写入同一
    /// 文件。offset **按字节精确定位**——与 `FileAccess::write_sink_chunk` 契约一致，
    /// 不做任何块尺寸取整（取整会把非对齐写静默落到错误位置，见读取侧 2026-07 事故）。
    pub async fn write_at(&self, offset: u64, data: &[u8]) -> AppResult<()> {
        let handle = {
            let guard = self.write_handle.lock().unwrap();
            guard
                .as_ref()
                .ok_or_else(|| AppError::Transfer("写入句柄已关闭".into()))?
                .clone()
        };

        let data = data.to_vec();

        tokio::task::spawn_blocking(move || write_all_at(&handle, &data, offset))
            .await?
            .map_err(|e: std::io::Error| AppError::StorageFailed(format!("写入分块失败: {e}")))
    }

    /// 关闭写入句柄。
    ///
    /// 发布前调用，确保数据已落盘且句柄释放（**Windows 下 rename 要求没有打开的句柄**）。
    /// 幂等，多次调用安全。
    pub fn close_write_handle(&self) {
        let mut guard = self.write_handle.lock().unwrap();
        *guard = None;
    }

    /// 发布：关闭写入句柄 → 把 `.part` 重命名到最终路径。
    ///
    /// **不做完整性校验**，理由见 [`super::sink_ops::publish`]。
    pub async fn publish(&self) -> AppResult<PathBuf> {
        self.close_write_handle();
        super::sink_ops::publish(self).await
    }

    /// 清理暂存文件（静默忽略错误）。
    ///
    /// 传输取消或失败时调用。**真正删掉盘上的半成品**是 `FileAccess::cleanup_sink` 契约的
    /// 一部分，不是可选优化——留着的话表现是「功能看起来正常，只是盘上慢慢堆残件」，
    /// 没有任何测试会红。
    pub async fn cleanup(&self) {
        self.close_write_handle();
        let _ = tokio::fs::remove_file(&self.part_path).await;
    }
}

impl fmt::Debug for PartFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PartFile")
            .field("part_path", &self.part_path)
            .field("final_path", &self.final_path)
            .field("size", &self.size)
            .finish()
    }
}

/// 由最终路径算出 `.part` 暂存路径。
///
/// 规则：在原扩展名后追加 `.part`（`readme.md` → `readme.md.part`）；
/// 无扩展名时直接追加（`Makefile` → `Makefile.part`）。
pub fn compute_part_path(final_path: &Path) -> PathBuf {
    final_path.with_extension(
        final_path
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".into()),
    )
}

/// Unix：`pwrite`，一次写完。
#[cfg(unix)]
fn write_all_at(file: &std::fs::File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(data, offset)
}

/// Windows：`seek_write` 不保证一次写完，循环到写满。
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
