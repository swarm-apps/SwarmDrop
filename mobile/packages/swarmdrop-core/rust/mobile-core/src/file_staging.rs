//! 接收侧的暂存区（staging）。
//!
//! 这是「暂存 → 发布」两阶段里的第一阶段：入站数据块先随机写进**应用私有目录**，
//! 整个文件收齐后再由 publish 搬到用户选定的目标位置（见 [`crate::file_access`]）。
//!
//! ## 为什么不直接写目标位置
//!
//! Android 上用户可以把接收目录设成系统公共目录，那是一个 SAF `content://` tree。
//! 往它上面做**随机写**有四条各自独立成立的理由：
//!
//! 1. **有些 fd 根本不能 seek。** `DocumentsProvider.openDocument` 允许返回管道式的
//!    描述符（`ParcelFileDescriptor.createPipe`），那种 fd 上 `FileChannel.position()`
//!    一律失败。接收是按 checkpoint 定位回写的，遇上这种 provider 就整条走不通——
//!    而具体碰上哪种 provider 由用户选的目录决定，我们无从预判。
//! 2. **SAF / FUSE 上的写本来就慢。** 每次写都要过 `/storage/emulated/0` 的 FUSE 与
//!    provider 进程，逐块随机写把这份开销乘上块数；顺序搬一次只付一遍。
//! 3. **不能让用户目录出现半成品。** 直接写目标意味着传到一半时文件管理器里就有一个
//!    长度不足的同名文件，看着像收到了。暂存则让「出现在用户目录」等价于「已收齐」。
//! 4. **续传要跨重启存活。** 暂存在应用私有区，与 checkpoint 同寿命、外人动不了；
//!    用户目录里的东西随时可能被用户或系统清理，恢复时的假设就不成立了。
//!
//! 所以**随机写只施加于本进程完全拥有的 fd**，外部位置只在 publish 时被顺序写一次。
//!
//! ### 归因更正（2026-08-10）
//!
//! 这段此前把上面的结论归给一条更强的断言：「SAF 的 fd 会在长时间写入期间被
//! provider 悄悄回收，下一次 `lseek` 直接 `EBADF`」，佐证是 2026-08-07 那次
//! 「311 MB 稳定在 45 MB 处崩掉」。**那个佐证是误诊。** 真实根因在 JS 那一侧：
//! `expo-file-system` 的 `forContentURI` 不持有 `ParcelFileDescriptor`，第一次 GC 的
//! finalizer 就把 fd 关了；本仓修这条的 pnpm patch 从未进过 Android 构建
//! （SDK 56 默认吃预编译 AAR，patch 打在源码上却没人编源码）。
//!
//! **结论没变，理由换了。** 保留这段的意义是：把一个可修的构建配置问题写成
//! 「SAF 天生不能 seek」，会让下一个人以为这条路无解——事实是它有解，只是我们
//! 出于上面四条**仍然**不走它。
//!
//! ## 顺带消除的一跳
//!
//! staging 恒为应用私有目录 = 普通 POSIX 路径，于是写入不必再经
//! uniffi → JS 线程 → JSI → Kotlin 绕一圈回来。JS 侧只保留它独有的能力
//! （`ContentResolver`/SAF），见 [`ForeignFileAccess`](crate::file_access::ForeignFileAccess)。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use swarmdrop_core::host::{CoreSaveLocation, FileSinkId, HostFileMetadata};
use swarmdrop_core::{AppError, AppResult};

/// 暂存目录名（挂在 `data_dir` 下，与 SQLite 同级）。
///
/// **不放 cache 目录**：系统在存储紧张时会清理它，而 staging 要跨「中断 → 用户过几天
/// 再点恢复」存活。放 `data_dir` 则与数据库同寿命——用户「清除数据」时一起消失，符合直觉。
const STAGING_DIR: &str = "staging";

/// 一条已打开的暂存文件。
struct StagingEntry {
    /// 写入句柄。`Arc` 是为了能 clone 出去交给 `spawn_blocking`，
    /// pwrite 只需 `&File`，多块并发写同一文件是安全的。
    file: Arc<std::fs::File>,
    path: PathBuf,
    /// publish 时需要它还原目标位置（`sink_id` 是 hash，反推不出来）。
    metadata: HostFileMetadata,
}

/// 应用私有的暂存区。
pub(crate) struct StagingArea {
    root: PathBuf,
    open: Mutex<HashMap<FileSinkId, StagingEntry>>,
}

impl StagingArea {
    /// 建目录**只在这里做一次**（进程启动期，一次 syscall）。
    ///
    /// 曾经放在 `open()` 里，于是每接收一个文件就 `mkdir`→EEXIST + `stat` 白跑一遍，
    /// 且跑在单线程 runtime 上。失败不在这里报——`open()` 打不开文件时的错误更具体。
    pub(crate) fn new(data_dir: &Path) -> Self {
        let root = data_dir.join(STAGING_DIR);
        let _ = std::fs::create_dir_all(&root);
        Self {
            root,
            open: Mutex::new(HashMap::new()),
        }
    }

    /// 建/开一条暂存文件。`truncate` 区分新传输与续传。
    ///
    /// 新传输清空同名残留；续传保留已有内容并按 checkpoint 随机写回去。
    pub(crate) async fn open(
        &self,
        metadata: HostFileMetadata,
        truncate: bool,
    ) -> AppResult<FileSinkId> {
        let sink_id = sink_id_for(&metadata)?;

        // 同一目标已在写：续传复用同一条，新传输拒绝——两个会话同时写一个目标位置
        // 只会互相破坏，这与桌面 `.part` 的独占语义一致。
        //
        // 检查与下面的 insert 之间隔着一次 `await`，理论上两个并发的新传输能同时通过
        // 这道检查、各自 truncate 同一个文件。实际到不了：core 的接收循环是严格串行的，
        // 而两个**不同会话**指向同一目标时，`create_sink`(truncate) 的第二次会在第一次
        // insert 之后到达。真要杜绝需要把占位写进表里再做 IO，那会让失败路径也得回滚占位
        // ——留着这条注释，比引入一个只在假想场景下有用的状态更划算。
        if let Some(existing) = self.open.lock().unwrap().get(&sink_id) {
            if truncate {
                return Err(AppError::Transfer(
                    "该接收目标已被另一次传输占用".to_string(),
                ));
            }
            if existing.metadata.size != metadata.size
                || existing.metadata.checksum != metadata.checksum
            {
                return Err(AppError::Transfer(
                    "续传的文件元信息与已打开的暂存不一致".to_string(),
                ));
            }
            return Ok(sink_id);
        }

        let path = self.root.join(&sink_id.0);
        // 与 `write_at` 同理：单线程 runtime 上不能同步开文件（慢存储上一次 open
        // 可达毫秒级，会卡住整个网络事件循环）。
        let open_path = path.clone();
        let file = tokio::task::spawn_blocking(move || {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(truncate)
                .open(&open_path)
        })
        .await
        .map_err(|e| AppError::StorageFailed(format!("打开暂存文件的任务失败: {e}")))?
        .map_err(|e| AppError::StorageFailed(format!("打开暂存文件失败: {e}")))?;

        self.open.lock().unwrap().insert(
            sink_id.clone(),
            StagingEntry {
                file: Arc::new(file),
                path,
                metadata,
            },
        );
        Ok(sink_id)
    }

    /// 定位写入一块（pwrite，不改文件偏移，多块并发安全）。
    ///
    /// **必须走 `spawn_blocking`**：mobile-core 从不自建 runtime，所有 async 都挤在
    /// async-compat 那根 `new_current_thread` 专用线程上（见
    /// `dev-notes/knowledge/rust-backend.md` 的「mobile-core 从不自建 tokio runtime」）。
    /// 在那根线程上同步写盘会把整个网络事件循环一起卡住。
    pub(crate) async fn write_at(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()> {
        let file = self.handle_of(sink)?;
        tokio::task::spawn_blocking(move || write_all_at(&file, &data, offset))
            .await
            .map_err(|e| AppError::StorageFailed(format!("暂存写入任务失败: {e}")))?
            .map_err(|e| AppError::StorageFailed(format!("暂存写入失败: {e}")))
    }

    /// 摘出一条暂存并关闭其句柄，交给 publish。
    ///
    /// 返回的 `metadata` 用于还原目标位置。摘出后即使 publish 失败也不放回——
    /// 续传路径会经 `open(.., truncate = false)` 重新接上盘上那个文件，
    /// 数据一个字节都不会丢。
    pub(crate) fn take(&self, sink: &FileSinkId) -> AppResult<(PathBuf, HostFileMetadata)> {
        let entry = self
            .open
            .lock()
            .unwrap()
            .remove(sink)
            .ok_or_else(|| AppError::Transfer(format!("暂存文件不存在: {}", sink.0)))?;
        // 句柄必须在 publish 之前落下：rename 之后再持有旧 fd 没有意义，
        // 而 SAF 分支要把这个文件整份读走。
        drop(entry.file);
        Ok((entry.path, entry.metadata))
    }

    /// 丢弃一条未发布的暂存：关句柄 + 删文件。**删除是契约的一部分**，不是可选优化
    /// （`FileAccess::cleanup_sink` 的文档写死了这条）。
    pub(crate) async fn discard(&self, sink: &FileSinkId) {
        let path = match self.open.lock().unwrap().remove(sink) {
            Some(entry) => {
                drop(entry.file);
                entry.path
            }
            // **不在表里也要删。** 发布失败时 `take()` 已经把它摘走了，可暂存文件还在盘上；
            // 若这里直接返回，取消路径的 `cleanup_sink` 就成了 no-op，一个全尺寸的暂存
            // 会永久残留（移动端的过期回收只扫**过期**会话，扫不到被取消的）。
            // 这里能补救，靠的是 `sink_id` 本身就是暂存文件名（见 `sink_id_for`）。
            None => self.root.join(&sink.0),
        };
        // 同 `open`：删文件也是阻塞 IO，不能占着单线程 runtime。
        let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(&path)).await;
    }

    fn handle_of(&self, sink: &FileSinkId) -> AppResult<Arc<std::fs::File>> {
        self.open
            .lock()
            .unwrap()
            .get(sink)
            .map(|entry| entry.file.clone())
            .ok_or_else(|| AppError::Transfer(format!("暂存文件不存在: {}", sink.0)))
    }
}

/// 暂存文件名 = `blake3(save_dir ‖ 0x00 ‖ relative_path)` 的 hex。
///
/// 三条要求同时被满足：
///
/// 1. **确定性**——续传只拿得到 [`HostFileMetadata`]（里面**没有** session_id），
///    必须能仅凭它重算出同一个位置。
/// 2. **唯一性**——同一目标目录下的同一相对路径映射到同一条，不同的则不撞。
/// 3. **可落盘**——扁平的 hex 名，不必递归建目录，也不受 SAF 上合法而本地文件系统
///    非法的字符、以及路径长度限制的影响。目录结构的信息在 `relative_path` 里，
///    由 publish 时重建。
///
/// `0x00` 分隔符不能省：没有它 `("/a/b", "c.txt")` 与 `("/a", "b/c.txt")` 会撞成同一条。
fn sink_id_for(metadata: &HostFileMetadata) -> AppResult<FileSinkId> {
    let Some(CoreSaveLocation::Path { path: save_dir }) = metadata.save_dir.as_ref() else {
        return Err(AppError::Transfer(
            "接收文件缺少保存目录：core 未注入 save_dir".to_string(),
        ));
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update(save_dir.as_bytes());
    hasher.update(&[0u8]);
    hasher.update(metadata.relative_path.as_bytes());
    Ok(FileSinkId(hasher.finalize().to_hex().to_string()))
}

/// Unix：pwrite，原子定位写入，不修改文件偏移量。
#[cfg(unix)]
fn write_all_at(file: &std::fs::File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(data, offset)
}

/// Windows：`seek_write` 同样不改文件偏移。
///
/// 移动端产物只在 Unix 上构建（iOS 要 macOS、Android 要 NDK），这条分支纯粹是为了
/// 让 `cargo check --workspace` 在 Windows 开发机上也能过——本 crate 是根 workspace
/// 的 member，会被一并 check。
#[cfg(windows)]
fn write_all_at(file: &std::fs::File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut written = 0;
    while written < data.len() {
        let n = file.seek_write(&data[written..], offset + written as u64)?;
        // 返回 0 既不是错误也不会推进游标——不拦就是死循环。桌面那份
        // （`src-tauri/src/host/file_sink.rs` 的同名函数）有这道护栏，
        // 这里最初漏了。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(save_dir: &str, relative_path: &str, size: u64) -> HostFileMetadata {
        HostFileMetadata {
            name: relative_path
                .rsplit('/')
                .next()
                .unwrap_or(relative_path)
                .into(),
            relative_path: relative_path.into(),
            size,
            modified_at: None,
            checksum: Some("checksum".into()),
            save_dir: Some(CoreSaveLocation::Path {
                path: save_dir.into(),
            }),
        }
    }

    /// 续传的全部前提：同样的元信息必须算出同一条暂存。
    #[test]
    fn sink_id_is_deterministic() {
        let a = sink_id_for(&metadata("/save", "docs/a.txt", 10)).unwrap();
        let b = sink_id_for(&metadata("/save", "docs/a.txt", 10)).unwrap();
        assert_eq!(a, b);
    }

    /// 分隔符存在的唯一理由。没有 `0x00`，这两组输入会拼成同一个字节串。
    #[test]
    fn sink_id_separates_dir_from_relative_path() {
        let a = sink_id_for(&metadata("/a/b", "c.txt", 10)).unwrap();
        let b = sink_id_for(&metadata("/a", "b/c.txt", 10)).unwrap();
        assert_ne!(a, b, "目录与相对路径的边界必须参与哈希");
    }

    #[test]
    fn sink_id_requires_save_dir() {
        let mut m = metadata("/save", "a.txt", 1);
        m.save_dir = None;
        assert!(sink_id_for(&m).is_err());
    }

    /// `create` 清空、`open_or_create` 保留——续传依赖后者不截断。
    #[tokio::test]
    async fn truncate_clears_while_reopen_preserves() {
        let dir = std::env::temp_dir().join("swarmdrop_staging_truncate");
        let _ = std::fs::remove_dir_all(&dir);
        let area = StagingArea::new(&dir);

        let m = metadata("/save", "a.bin", 8);
        let sink = area.open(m.clone(), true).await.unwrap();
        area.write_at(&sink, 0, vec![1u8; 8]).await.unwrap();
        let (path, _) = area.take(&sink).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), vec![1u8; 8]);

        // 续传：不截断，旧内容还在
        let sink = area.open(m.clone(), false).await.unwrap();
        area.write_at(&sink, 0, vec![2u8; 4]).await.unwrap();
        let (path, _) = area.take(&sink).unwrap();
        let content = std::fs::read(&path).unwrap();
        assert_eq!(&content[..4], &[2u8; 4], "写入的部分被覆盖");
        assert_eq!(&content[4..], &[1u8; 4], "未写入的部分必须保留");

        // 新传输：截断，旧内容清空
        let sink = area.open(m, true).await.unwrap();
        let (path, _) = area.take(&sink).unwrap();
        assert!(std::fs::read(&path).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// pwrite 语义：按 offset 定位，不受写入顺序影响。
    #[tokio::test]
    async fn writes_land_at_their_offset_regardless_of_order() {
        let dir = std::env::temp_dir().join("swarmdrop_staging_offset");
        let _ = std::fs::remove_dir_all(&dir);
        let area = StagingArea::new(&dir);

        let sink = area
            .open(metadata("/save", "a.bin", 6), true)
            .await
            .unwrap();
        area.write_at(&sink, 3, vec![0xBB; 3]).await.unwrap();
        area.write_at(&sink, 0, vec![0xAA; 3]).await.unwrap();
        let (path, _) = area.take(&sink).unwrap();

        assert_eq!(
            std::fs::read(&path).unwrap(),
            vec![0xAA, 0xAA, 0xAA, 0xBB, 0xBB, 0xBB]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn discard_removes_the_file() {
        let dir = std::env::temp_dir().join("swarmdrop_staging_discard");
        let _ = std::fs::remove_dir_all(&dir);
        let area = StagingArea::new(&dir);

        let sink = area
            .open(metadata("/save", "a.bin", 4), true)
            .await
            .unwrap();
        area.write_at(&sink, 0, vec![7u8; 4]).await.unwrap();
        let path = area.root.join(&sink.0);
        assert!(path.exists());

        area.discard(&sink).await;
        assert!(!path.exists(), "cleanup 必须真删掉暂存产物");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 两个会话同时写同一个目标位置只会互相破坏。
    #[tokio::test]
    async fn concurrent_new_transfers_to_same_target_are_rejected() {
        let dir = std::env::temp_dir().join("swarmdrop_staging_conflict");
        let _ = std::fs::remove_dir_all(&dir);
        let area = StagingArea::new(&dir);

        let m = metadata("/save", "a.bin", 4);
        area.open(m.clone(), true).await.unwrap();
        assert!(area.open(m, true).await.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
