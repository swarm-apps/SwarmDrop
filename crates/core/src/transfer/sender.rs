//! 发送方会话
//!
//! 管理单个发送传输的生命周期：响应 ChunkRequest、处理 Complete/Cancel。
//! 文件读取通过 [`FileAccess`] trait 完成，加密使用 [`TransferCrypto`]。
//! 使用 `Arc<std::sync::Mutex<ProgressTracker>>` 实现并发安全的进度追踪。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::io::AsyncReadExt;
use swarm_p2p_core::DataChannel;
use swarm_p2p_core::libp2p::PeerId;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{EventBus, FileAccess};
use crate::protocol::{FileInfo, FileRange, TransferResponse};
use crate::transfer::crypto::TransferCrypto;
use crate::transfer::data_frame::{
    TransferDataFrame, TransferDataRole, manifest_digest, read_frame, write_frame,
};
use crate::transfer::manager::PreparedFile;
use crate::transfer::progress::{FileDesc, ProgressTracker, RuntimeTransferDirection};
use crate::transfer::{CHUNK_SIZE, calc_total_chunks};
use crate::{AppError, AppResult};

/// 发送方会话
pub struct SendSession {
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
    /// 进度追踪器（Arc<Mutex> 供并发 ChunkRequest 任务共享）
    progress: Arc<Mutex<ProgressTracker>>,
    /// 取消令牌
    cancel_token: CancellationToken,
    /// 会话创建时间（用于统计传输耗时）
    created_at: Instant,
    /// 最后活动时间戳（毫秒，从 created_at 起算，用于空闲超时清理）
    last_activity_ms: Arc<AtomicU64>,
}

impl SendSession {
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

    /// 处理 ChunkRequest：读取文件分块 → 加密 → 上报进度 → 返回 Chunk 响应
    pub async fn handle_chunk_request(
        &self,
        file_id: u32,
        chunk_index: u32,
    ) -> AppResult<TransferResponse> {
        if self.cancel_token.is_cancelled() {
            return Err(AppError::Transfer("传输已取消".into()));
        }

        let file = self
            .files
            .iter()
            .find(|f| f.file_id == file_id)
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: file_id={file_id}")))?;

        // 通过 FileAccess 异步读取分块
        let offset = chunk_index as u64 * CHUNK_SIZE as u64;
        let remaining = file.size.saturating_sub(offset);
        if remaining == 0 && file.size != 0 {
            return Err(AppError::Transfer(format!(
                "chunk_index 超出范围: offset={offset}, file_size={}",
                file.size
            )));
        }
        let length = (remaining as usize).min(CHUNK_SIZE);
        let plaintext = self
            .file_access
            .read_source_chunk(&file.source_id, offset, length)
            .await?;

        let plaintext_len = plaintext.len() as u64;

        // 加密
        let data = self
            .crypto
            .encrypt_chunk(&self.session_id, file_id, chunk_index, &plaintext)
            .map_err(|e| AppError::Transfer(format!("加密失败: {e}")))?;

        // 更新最后活动时间戳
        self.last_activity_ms.store(
            self.created_at.elapsed().as_millis() as u64,
            Ordering::Relaxed,
        );

        // 累加进度（在锁内做最少操作，拿到事件再 publish）
        let progress_event = {
            let mut p = self
                .progress
                .lock()
                .map_err(|_| AppError::Transfer("ProgressTracker 锁中毒".into()))?;
            p.add_bytes(plaintext_len);
            p.update_file_chunk(file_id, plaintext_len);
            p.progress_event()
        };
        if let Some(event) = progress_event {
            let _ = self
                .event_bus
                .publish(crate::host::CoreEvent::TransferProgress { event })
                .await;
        }

        // 计算 is_last
        let total_chunks = calc_total_chunks(file.size);
        let is_last = chunk_index + 1 >= total_chunks;

        Ok(TransferResponse::Chunk {
            session_id: self.session_id,
            file_id,
            chunk_index,
            data,
            is_last,
        })
    }

    /// 处理 Complete：记录日志，会话将由 TransferManager 清理
    pub fn handle_complete(&self) {
        info!(
            "Transfer complete acknowledged: session={}",
            self.session_id
        );
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
            loop {
                if self.cancel_token.is_cancelled() {
                    return Err(AppError::Transfer("传输已取消".into()));
                }

                match read_frame(&mut reader).await? {
                    Some(TransferDataFrame::Ack {
                        session_id,
                        epoch: ack_epoch,
                        ..
                    }) if session_id == self.session_id && ack_epoch == epoch => {}
                    Some(TransferDataFrame::Finish {
                        session_id,
                        epoch: finish_epoch,
                    }) if session_id == self.session_id && finish_epoch == epoch => return Ok(()),
                    Some(TransferDataFrame::BlockRequest { .. }) => {
                        return Err(AppError::Transfer(
                            "当前 transfer-data 流不支持 BlockRequest 重传".into(),
                        ));
                    }
                    Some(TransferDataFrame::Abort { reason, .. }) => {
                        return Err(AppError::Transfer(format!("对端中止传输: {reason}")));
                    }
                    Some(other) => {
                        return Err(AppError::Transfer(format!(
                            "发送方收到意外 data frame: {other:?}"
                        )));
                    }
                    None => return Err(AppError::Transfer("data channel 在完成前关闭".into())),
                }
            }
        };

        tokio::try_join!(writer_task, reader_task)?;
        Ok(())
    }

    fn file_manifest(&self) -> Vec<FileInfo> {
        self.files
            .iter()
            .map(|f| FileInfo {
                file_id: f.file_id,
                name: f.name.clone(),
                relative_path: f.relative_path.clone(),
                size: f.size,
                checksum: f.checksum.clone(),
            })
            .collect()
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
