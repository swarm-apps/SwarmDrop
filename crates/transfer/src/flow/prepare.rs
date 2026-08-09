//! 发送方 prepare 阶段：**一遍**流式读同时产出 checksum 与 bao outboard，并推进度事件。
//!
//! 这里只挂载 `TransferManager::prepare` 一个方法，结构体定义和其他生命周期方法
//! 仍在 [`crate::manager`]。
//!
//! # 为什么是一遍
//!
//! 2026-08 之前这里读两遍：先逐 chunk 跑一遍扁平 `blake3::Hasher` 得 `checksum`，
//! 再调 `build_outboard_from_source` 重读一遍建验签树，靠 `debug_assert_eq!` 互证。
//! 但 bao 树根**就是**标准 blake3 整文件 hash，第一遍在语义上是纯冗余——它当时唯一的
//! 作用是产出进度事件（所有 emit 都写在它的循环里，建树那遍一个事件都不发）。
//!
//! 现在进度经 [`ReadProgress`] 挂在建树那遍的 reader 上，于是：读盘量减半、进度覆盖
//! 全部真实工作量（不再有「进度条走完后再静默等一倍时间」）、且 checksum 与 outboard
//! 同源——「两遍之间源文件被改」产出互不匹配二者的那类事故在构造上不可能。

use std::sync::Arc;

use n0_future::time::Instant;

use uuid::Uuid;

use crate::HostEnumeratedFile;
use crate::bao::ReadProgress;
use crate::events::{TransferEvent, TransferEventSink};
use crate::manager::{PreparedFile, PreparedTransfer, TransferManager};
use crate::progress::PrepareProgressEvent;
use crate::{AppError, AppResult};

/// PrepareProgress 的节流间隔，避免大文件刷屏。
///
/// 与 [`crate::progress`] 里传输进度用的那个同值——两者在 UI 上是同一条进度条的前后两段，
/// 节奏漂移用户直接看得见。
const PROGRESS_THROTTLE: std::time::Duration = std::time::Duration::from_millis(200);

/// 把 bao 构建的读取进度翻译成 [`TransferEvent::PrepareProgress`]。
///
/// **跨文件存活**：节流时刻与已完成字节都是整批的量，每文件新建一个会让小文件批量
/// 刷屏、也会让进度在文件边界上归零。
struct PrepareReporter<'a> {
    events: &'a Arc<dyn TransferEventSink>,
    prepared_id: Uuid,
    /// 当前正在读的文件名，随外层循环推进。
    current_file: String,
    /// 已完成的文件数（= 当前文件的 file_id）。
    completed_files: u32,
    total_files: u32,
    /// **此前**已完成文件的累计字节，不含当前文件。
    completed_bytes: u64,
    total_bytes: u64,
    /// `None` = 还没发过，**第一条必发**（与 `ProgressTracker::progress_event` 同语义）。
    ///
    /// 曾初始化为 `Instant::now()`，于是开头 200ms 一条都不发：小批量准备在 200ms 内跑完时
    /// 用户先看到一段空白、然后直接跳到收尾那条 100%。
    last_emit: Option<Instant>,
}

impl PrepareReporter<'_> {
    /// 按当前字段发一条事件，**不判节流**。
    async fn emit(&self, bytes_in_file: u64) {
        // 事件总线抖动不该中断一次正常的准备：吞掉。
        let _ = self
            .events
            .emit(TransferEvent::PrepareProgress {
                event: PrepareProgressEvent {
                    prepared_id: self.prepared_id,
                    current_file: self.current_file.clone(),
                    completed_files: self.completed_files,
                    total_files: self.total_files,
                    bytes_hashed: self.completed_bytes + bytes_in_file,
                    total_bytes: self.total_bytes,
                },
            })
            .await;
    }

    /// 整批结束时那一条，不受节流。
    ///
    /// **`current_file` 保持最后一个文件的名字，不置空。** 曾经置空，代价落在三端 UI 上：
    /// 各写一处 `currentFile ? <文件名行> : null`，于是进度条冲到 100% 的同一帧里文件名
    /// 行凭空消失、整条操作栏高度跳一下；Web 那边还为「没有文件名」这个只存在几毫秒的
    /// 状态多养了一条 msgid。哨兵值的代价从来不在产生它的地方。
    async fn emit_final(&mut self) {
        self.completed_files = self.total_files;
        self.completed_bytes = self.total_bytes;
        self.emit(0).await;
    }
}

impl ReadProgress for PrepareReporter<'_> {
    async fn on_read(&mut self, bytes_in_file: u64) {
        let now = Instant::now();
        if self
            .last_emit
            .is_some_and(|last| now.duration_since(last) < PROGRESS_THROTTLE)
        {
            return;
        }
        self.last_emit = Some(now);
        self.emit(bytes_in_file).await;
    }
}

impl TransferManager {
    /// 准备发送：一遍流式读产出 checksum + bao outboard，并推进度事件。
    ///
    /// - `prepared_id` 由 caller 生成，宿主据此关联进度事件
    /// - 读取通过 `FileAccess::read_source_chunk` 走，平台无关
    /// - PrepareProgress 事件按 200ms 节流推送
    pub async fn prepare(
        &self,
        prepared_id: Uuid,
        entries: Vec<HostEnumeratedFile>,
    ) -> AppResult<PreparedTransfer> {
        if entries.is_empty() {
            return Err(AppError::Transfer("文件列表为空".into()));
        }

        let total_files = entries.len() as u32;
        let total_bytes: u64 = entries.iter().map(|e| e.size).sum();
        let mut files = Vec::with_capacity(entries.len());

        let mut reporter = PrepareReporter {
            events: &self.events,
            prepared_id,
            current_file: String::new(),
            completed_files: 0,
            total_files,
            completed_bytes: 0,
            total_bytes,
            last_emit: None,
        };

        for (idx, entry) in entries.into_iter().enumerate() {
            let file_id = idx as u32;
            reporter.current_file = entry.name.clone();
            reporter.completed_files = file_id;

            // 一遍流式读：root == 标准 blake3 整文件 hash == FileInfo.checksum。
            // 宿主违约（返回长度不等于请求长度）在 reader 内即响错，带文件名可归因。
            let (root, outboard) = crate::bao::build_outboard_from_source_with_progress(
                &self.file_access,
                &entry.source_id,
                entry.size,
                &entry.relative_path,
                &mut reporter,
            )
            .await?;

            reporter.completed_bytes += entry.size;

            files.push(PreparedFile {
                file_id,
                name: entry.name,
                relative_path: entry.relative_path,
                source_id: entry.source_id,
                size: entry.size,
                checksum: root.to_hex().to_string(),
                outboard,
            });
        }

        reporter.emit_final().await;

        let prepared = PreparedTransfer {
            prepared_id,
            files,
            total_size: total_bytes,
            created_at: Instant::now(),
        };

        self.prepared.insert(prepared.prepared_id, prepared.clone());
        Ok(prepared)
    }
}
