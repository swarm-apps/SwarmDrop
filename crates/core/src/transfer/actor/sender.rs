//! 发送方 actor（SenderActor）
//!
//! 管理单个发送传输的生命周期：经 data-channel 推送文件块、处理 Cancel。
//! 文件读取通过 [`FileAccess`] trait 完成，加密使用 [`TransferCrypto`]。
//! 使用 `Arc<std::sync::Mutex<ProgressTracker>>` 实现并发安全的进度追踪。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::io::AsyncReadExt;
use sea_orm::DatabaseConnection;
use swarm_p2p_core::DataChannel;
use swarm_p2p_core::libp2p::PeerId;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{CoreEvent, EventBus, FileAccess};
use crate::protocol::{FileInfo, FileRange};
use crate::transfer::CHUNK_SIZE;
use crate::transfer::coordinator::{
    ActorReport, CoordinatorInput, NetworkSignal, TransferCoordinator,
};
use crate::transfer::epoch::EpochGuard;
use crate::transfer::manager::PreparedFile;
use crate::transfer::progress::{
    FileDesc, ProgressTracker, RuntimeTransferDirection, TransferCompleteEvent,
};
use crate::transfer::wire::crypto::TransferCrypto;
use crate::transfer::wire::data_frame::{
    TransferDataFrame, TransferDataRole, manifest_digest, read_frame, write_frame,
};
use crate::{AppError, AppResult};

/// 发送方 actor（SenderActor）
pub struct SenderActor {
    /// 传输会话 ID
    pub session_id: Uuid,
    /// 对端 PeerId（暂停时需要通知对端）
    pub peer_id: PeerId,
    /// 准备好的文件列表（含文件来源）
    files: Vec<PreparedFile>,
    /// 加密器
    crypto: TransferCrypto,
    /// 文件访问 trait（host 实现，桌面=本地路径，RN=expo-fs callback）
    file_access: Arc<dyn FileAccess>,
    /// 事件总线（推送进度等给 host）
    event_bus: Arc<dyn EventBus>,
    /// 进度追踪器（Arc<Mutex> 供 data-channel 推送任务共享）
    progress: Arc<Mutex<ProgressTracker>>,
    /// 取消令牌
    cancel_token: CancellationToken,
    /// 会话创建时间（用于统计传输耗时）
    created_at: Instant,
    /// 最后活动时间戳（毫秒，从 created_at 起算，用于空闲超时清理）
    last_activity_ms: Arc<AtomicU64>,
}

impl SenderActor {
    pub fn new(
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<PreparedFile>,
        key: &[u8; 32],
        file_access: Arc<dyn FileAccess>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self::new_inner(
            session_id,
            peer_id,
            files,
            key,
            file_access,
            event_bus,
            &HashMap::new(),
        )
    }

    /// 断点续传专用构造函数
    ///
    /// `resume_state` 为每个文件的已完成 chunk 数和已传输字节数（从 DB 读取），
    /// 使 ProgressTracker 从正确的位置开始计数。
    pub fn new_with_resume(
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<PreparedFile>,
        key: &[u8; 32],
        file_access: Arc<dyn FileAccess>,
        event_bus: Arc<dyn EventBus>,
        resume_state: &HashMap<u32, (u32, u64)>,
    ) -> Self {
        Self::new_inner(
            session_id,
            peer_id,
            files,
            key,
            file_access,
            event_bus,
            resume_state,
        )
    }

    fn new_inner(
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<PreparedFile>,
        key: &[u8; 32],
        file_access: Arc<dyn FileAccess>,
        event_bus: Arc<dyn EventBus>,
        resume_state: &HashMap<u32, (u32, u64)>,
    ) -> Self {
        let total_bytes: u64 = files.iter().map(|f| f.size).sum();
        let total_files = files.len();

        let mut tracker = ProgressTracker::new(
            session_id,
            RuntimeTransferDirection::Send,
            total_bytes,
            total_files,
        );

        let file_descs: Vec<FileDesc> = files
            .iter()
            .map(|f| FileDesc {
                file_id: f.file_id,
                name: f.name.clone(),
                size: f.size,
            })
            .collect();
        tracker.init_files_with_resume(&file_descs, resume_state);

        Self {
            session_id,
            peer_id,
            files,
            crypto: TransferCrypto::new(key),
            file_access,
            event_bus,
            progress: Arc::new(Mutex::new(tracker)),
            cancel_token: CancellationToken::new(),
            created_at: Instant::now(),
            last_activity_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 获取传输耗时（毫秒）
    pub fn elapsed_ms(&self) -> u64 {
        self.created_at.elapsed().as_millis() as u64
    }

    /// 获取已发送总字节数（从 ProgressTracker 读取）
    pub fn total_bytes_sent(&self) -> u64 {
        self.progress.lock().map_or(0, |p| p.transferred_bytes())
    }

    /// 获取每个文件的已传输进度（用于暂停时持久化到 DB）
    ///
    /// 返回 `Vec<(file_id, chunks_done, transferred_bytes)>`
    pub fn get_file_progress(&self) -> Vec<(u32, u32, u64)> {
        self.progress
            .lock()
            .map(|p| p.get_file_progress())
            .unwrap_or_default()
    }

    /// 处理 Cancel：取消所有进行中的操作
    pub fn handle_cancel(&self) {
        warn!("Transfer cancelled by peer: session={}", self.session_id);
        self.cancel_token.cancel();
    }

    /// 获取取消令牌（供外部检查是否已取消）
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }

    /// 主动取消
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// 将本发送会话绑定到一条 data channel，并按 fetch_plan 连续推送数据。
    pub async fn run_data_channel(
        &self,
        epoch: i64,
        channel: DataChannel,
        fetch_plan: Vec<FileRange>,
    ) -> AppResult<()> {
        let manifest = self.file_manifest();
        let plan = fetch_plan;
        let (mut reader, mut writer) = channel.into_stream().split();

        let writer_task = async {
            write_frame(
                &mut writer,
                &TransferDataFrame::Hello {
                    session_id: self.session_id,
                    epoch,
                    role: TransferDataRole::Sender,
                    manifest_digest: manifest_digest(&manifest),
                    fetch_plan: plan.clone(),
                },
            )
            .await?;

            for range in plan {
                self.write_range(&mut writer, epoch, range).await?;
            }

            write_frame(
                &mut writer,
                &TransferDataFrame::Finish {
                    session_id: self.session_id,
                    epoch,
                },
            )
            .await
        };

        let reader_task = async {
            // 接收方收完并 finalize 后回一帧 Finish 作为完成确认（已无逐块 Ack），读到它即完成。
            // 空闲等待时响应取消，避免 cancel 后干等到对端 Finish 或超时。
            let frame = tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    return Err(AppError::Transfer("传输已取消".into()));
                }
                frame = read_frame(&mut reader) => frame?,
            };
            match frame {
                Some(TransferDataFrame::Finish {
                    session_id,
                    epoch: finish_epoch,
                }) if session_id == self.session_id && EpochGuard::matches(finish_epoch, epoch) => {
                    Ok(())
                }
                Some(TransferDataFrame::Abort { reason, .. }) => {
                    Err(AppError::Transfer(format!("对端中止传输: {reason}")))
                }
                Some(other) => Err(AppError::Transfer(format!(
                    "发送方收到意外 data frame: {other:?}"
                ))),
                None => Err(AppError::Transfer("data channel 在完成前关闭".into())),
            }
        };

        tokio::try_join!(writer_task, reader_task)?;
        Ok(())
    }

    /// 发送数据面正常结束的终态副作用（与接收方 `finish_data_channel` 对称）。
    ///
    /// session 终态经状态机 `dispatch(Actor{epoch, Completed})`，享受 epoch + terminal
    /// 不可逆守卫（旧 epoch / 已取消的会话不被覆盖）；仅真正转入 completed 才发完成事件。
    /// coordinator/event_bus 由 data_plane 传入（actor 自身只在完成回调点用一次，不持有）。
    pub async fn on_completed(
        &self,
        epoch: i64,
        coordinator: &TransferCoordinator,
        event_bus: &dyn EventBus,
    ) {
        match coordinator
            .dispatch(
                self.session_id,
                CoordinatorInput::Actor {
                    epoch,
                    report: ActorReport::Completed,
                },
            )
            .await
        {
            Ok(Some(_)) => {
                // 复用 ProgressTracker::complete_event（与接收方 finish_data_channel 对称），
                // 不再手搓 TransferCompleteEvent。锁中毒（极罕见）则跳过完成事件。
                let event = self.progress.lock().ok().map(|p| p.complete_event(None));
                if let Some(event) = event {
                    let _ = event_bus
                        .publish(CoreEvent::TransferCompleted { event })
                        .await;
                }
            }
            Ok(None) => info!(
                "发送完成被状态机忽略（已 terminal / 旧 epoch）: session={}",
                self.session_id
            ),
            Err(e) => warn!("dispatch 发送完成失败: session={}, {e}", self.session_id),
        }
    }

    /// 发送数据面因非取消错误中断的终态副作用：先持久化已发进度（供续传），
    /// 再经状态机 `dispatch(Network{epoch, Interrupted})` 转 suspended/recoverable。
    pub async fn on_interrupted(
        &self,
        epoch: i64,
        coordinator: &TransferCoordinator,
        db: &DatabaseConnection,
    ) {
        let progress = self.get_file_progress();
        let _ =
            crate::database::ops::save_sender_file_progress(db, self.session_id, &progress).await;
        let _ = coordinator
            .dispatch(
                self.session_id,
                CoordinatorInput::Network {
                    epoch,
                    signal: NetworkSignal::Interrupted,
                },
            )
            .await;
    }

    fn file_manifest(&self) -> Vec<FileInfo> {
        self.files.iter().map(FileInfo::from).collect()
    }

    async fn write_range<W>(&self, stream: &mut W, epoch: i64, range: FileRange) -> AppResult<()>
    where
        W: futures::io::AsyncWrite + Unpin,
    {
        let file = self
            .files
            .iter()
            .find(|f| f.file_id == range.file_id)
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: file_id={}", range.file_id)))?;

        let end = range
            .offset
            .checked_add(range.length)
            .ok_or_else(|| AppError::Transfer("fetch range 溢出".into()))?;
        if end > file.size {
            return Err(AppError::Transfer(format!(
                "fetch range 超出文件大小: file_id={}, end={}, size={}",
                range.file_id, end, file.size
            )));
        }

        if file.size == 0 && range.offset == 0 && range.length == 0 {
            self.write_block(stream, epoch, file, 0, 0).await?;
            return Ok(());
        }

        let mut offset = range.offset;
        while offset < end {
            if self.cancel_token.is_cancelled() {
                return Err(AppError::Transfer("传输已取消".into()));
            }
            let len = ((end - offset) as usize).min(CHUNK_SIZE);
            self.write_block(stream, epoch, file, offset, len).await?;
            offset += len as u64;
        }

        Ok(())
    }

    async fn write_block<W>(
        &self,
        stream: &mut W,
        epoch: i64,
        file: &PreparedFile,
        offset: u64,
        length: usize,
    ) -> AppResult<()>
    where
        W: futures::io::AsyncWrite + Unpin,
    {
        let plaintext = self
            .file_access
            .read_source_chunk(&file.source_id, offset, length)
            .await?;
        let plaintext_len = plaintext.len() as u64;
        let chunk_index = (offset / CHUNK_SIZE as u64) as u32;
        let ciphertext = self
            .crypto
            .encrypt_chunk(&self.session_id, file.file_id, chunk_index, &plaintext)
            .map_err(|e| AppError::Transfer(format!("加密失败: {e}")))?;

        write_frame(
            stream,
            &TransferDataFrame::BlockData {
                session_id: self.session_id,
                epoch,
                range: FileRange {
                    file_id: file.file_id,
                    offset,
                    length: plaintext_len,
                },
                ciphertext,
            },
        )
        .await?;

        self.last_activity_ms.store(
            self.created_at.elapsed().as_millis() as u64,
            Ordering::Relaxed,
        );

        let progress_event = {
            let mut p = self
                .progress
                .lock()
                .map_err(|_| AppError::Transfer("ProgressTracker 锁中毒".into()))?;
            p.add_bytes(plaintext_len);
            p.update_file_chunk(file.file_id, plaintext_len);
            p.progress_event()
        };
        if let Some(event) = progress_event {
            let _ = self
                .event_bus
                .publish(crate::host::CoreEvent::TransferProgress { event })
                .await;
        }

        Ok(())
    }

    /// 返回自上次活动以来的空闲时间（毫秒）
    pub fn idle_ms(&self) -> u64 {
        let elapsed = self.created_at.elapsed().as_millis() as u64;
        let last = self.last_activity_ms.load(Ordering::Relaxed);
        elapsed.saturating_sub(last)
    }
}
