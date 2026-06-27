//! 接收方会话
//!
//! 管理单个接收传输的生命周期：并发拉取分块、解密写入、校验、完成确认。
//! 文件 I/O 全部通过 [`FileAccess`] trait 完成，加密使用 [`TransferCrypto`]。
//! 使用 Semaphore 控制并发度（8 并发），CancellationToken 支持取消。

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use futures::io::AsyncReadExt;
use sea_orm::DatabaseConnection;
use swarm_p2p_core::libp2p::PeerId;
use swarm_p2p_core::libp2p::Stream;
use tokio::sync::{Mutex, Semaphore, mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::host::{
    CoreEvent, CoreSaveLocation, EventBus, FileAccess, FileSinkId, HostFileMetadata,
};
use crate::protocol::{
    AppNetClient, AppRequest, AppResponse, FileInfo, FileRange, TransferRequest, TransferResponse,
};
use crate::transfer::coordinator::TransferCoordinator;
use crate::transfer::crypto::TransferCrypto;
use crate::transfer::data_frame::{TransferDataFrame, manifest_digest, read_frame, write_frame};
use crate::transfer::progress::{
    FileDesc, ProgressTracker, RuntimeTransferDirection, TransferDbErrorEvent,
};
use crate::transfer::{CHUNK_SIZE, calc_total_chunks};
use crate::{AppError, AppResult};

/// 最大并发拉取数
const MAX_CONCURRENT_CHUNKS: usize = 8;

/// 单个分块最大重试次数
const MAX_CHUNK_RETRIES: u32 = 3;

/// 重试基础延迟
const RETRY_DELAY_BASE_MS: u64 = 500;

/// 每完成多少个 chunk 刷写一次 bitmap checkpoint 到 DB
const CHECKPOINT_INTERVAL: u32 = 10;

/// 接收方会话
pub struct ReceiveSession {
    /// 传输会话 ID
    pub session_id: Uuid,
    /// 发送方 PeerId
    pub peer_id: PeerId,
    /// 文件列表
    files: Vec<FileInfo>,
    /// 总大小
    total_size: u64,
    /// 文件访问 trait
    file_access: Arc<dyn FileAccess>,
    /// 事件总线
    event_bus: Arc<dyn EventBus>,
    /// 数据库连接（断点续传 checkpoint 持久化）
    db: Arc<DatabaseConnection>,
    /// 生命周期协调器（接收方自身 complete/fail 后发 projection，消除收发不对称）
    coordinator: Arc<TransferCoordinator>,
    /// 保存位置（用于完成事件 payload，host 自己定义语义）
    save_location: CoreSaveLocation,
    /// 加密器
    crypto: Arc<TransferCrypto>,
    /// 网络客户端
    client: AppNetClient,
    /// 取消令牌
    cancel_token: CancellationToken,
    /// 已创建的临时文件（用于取消时清理）
    created_sinks: Mutex<Vec<FileSinkId>>,
    /// 断点续传初始 bitmap（file_id → completed_chunks bitmap），首次传输为空
    initial_bitmaps: HashMap<u32, Vec<u8>>,
    /// 传输完成信号（start_pulling 结束后发送 true）
    finished_tx: watch::Sender<bool>,
}

impl ReceiveSession {
    #[expect(clippy::too_many_arguments, reason = "传输会话初始化需要完整上下文")]
    pub fn new(
        session_id: Uuid,
        peer_id: PeerId,
        files: Vec<FileInfo>,
        total_size: u64,
        file_access: Arc<dyn FileAccess>,
        event_bus: Arc<dyn EventBus>,
        db: Arc<DatabaseConnection>,
        coordinator: Arc<TransferCoordinator>,
        save_location: CoreSaveLocation,
        key: &[u8; 32],
        client: AppNetClient,
        initial_bitmaps: HashMap<u32, Vec<u8>>,
    ) -> Self {
        let (finished_tx, _) = watch::channel(false);
        Self {
            session_id,
            peer_id,
            files,
            total_size,
            file_access,
            event_bus,
            db,
            coordinator,
            save_location,
            crypto: Arc::new(TransferCrypto::new(key)),
            client,
            cancel_token: CancellationToken::new(),
            created_sinks: Mutex::new(Vec::new()),
            initial_bitmaps,
            finished_tx,
        }
    }

    /// 启动后台拉取任务
    ///
    /// 逐文件、并发分块拉取 → 解密 → 写入 → 校验 → 最终化。
    /// 所有文件完成后发送 Complete 消息给发送方。
    /// `on_finish` 在任务结束（成功或失败）后调用，用于清理 DashMap 中的会话引用。
    pub fn start_pulling<F>(self: Arc<Self>, on_finish: F)
    where
        F: FnOnce(&Uuid) + Send + 'static,
    {
        tokio::spawn(async move {
            match self.run_transfer().await {
                Ok(true) => info!(
                    "Transfer completed successfully: session={}",
                    self.session_id
                ),
                Ok(false) => info!("Transfer cancelled: session={}", self.session_id),
                Err(e) => error!("Transfer failed: session={}, error={}", self.session_id, e),
            }

            let _ = self.finished_tx.send(true);
            on_finish(&self.session_id);
        });
    }

    /// 等待传输任务完成（含最终 bitmap 刷写）
    pub async fn wait_finished(&self) {
        let mut rx = self.finished_tx.subscribe();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }

    pub fn expected_manifest_digest(&self) -> [u8; 32] {
        manifest_digest(&self.files)
    }

    pub fn validate_fetch_plan(&self, fetch_plan: &[FileRange]) -> AppResult<()> {
        for range in fetch_plan {
            let file = self
                .files
                .iter()
                .find(|file| file.file_id == range.file_id)
                .ok_or_else(|| {
                    AppError::Transfer(format!("fetch_plan 引用未知文件: {}", range.file_id))
                })?;
            let end = range
                .offset
                .checked_add(range.length)
                .ok_or_else(|| AppError::Transfer("fetch_plan range 溢出".into()))?;
            if end > file.size {
                return Err(AppError::Transfer(format!(
                    "fetch_plan range 超出文件大小: file_id={}, end={}, size={}",
                    range.file_id, end, file.size
                )));
            }
            if file.size > 0 && range.length == 0 {
                return Err(AppError::Transfer(format!(
                    "非空文件的 fetch_plan range 长度为 0: file_id={}",
                    range.file_id
                )));
            }
        }
        Ok(())
    }

    /// 启动 data-channel 接收任务。
    ///
    /// Hello 已由 `TransferManager` 入站路由读取和校验；这里从 BlockData / Finish 开始读。
    pub fn start_data_channel<F>(
        self: Arc<Self>,
        epoch: i64,
        stream: Stream,
        fetch_plan: Vec<FileRange>,
        on_finish: F,
    ) where
        F: FnOnce(&Uuid) + Send + 'static,
    {
        tokio::spawn(async move {
            let outcome = self.run_data_channel(epoch, stream, fetch_plan).await;
            match &outcome {
                Ok(true) => info!(
                    "Data-channel receive completed: session={}",
                    self.session_id
                ),
                Ok(false) => info!(
                    "Data-channel receive cancelled: session={}",
                    self.session_id
                ),
                Err(e) => {
                    if self.cancel_token.is_cancelled() {
                        info!(
                            "Data-channel receive stopped after cancellation: session={}",
                            self.session_id
                        );
                    } else {
                        warn!(
                            "Data-channel receive interrupted: session={}, error={}",
                            self.session_id, e
                        );
                        let _ = self
                            .coordinator
                            .dispatch(
                                self.session_id,
                                crate::transfer::coordinator::CoordinatorInput::Network {
                                    epoch,
                                    signal:
                                        crate::transfer::coordinator::NetworkSignal::Interrupted,
                                },
                            )
                            .await;
                    }
                }
            }

            let _ = self.finished_tx.send(true);
            on_finish(&self.session_id);
        });
    }

    async fn run_data_channel(
        self: &Arc<Self>,
        epoch: i64,
        stream: Stream,
        fetch_plan: Vec<FileRange>,
    ) -> AppResult<bool> {
        self.validate_fetch_plan(&fetch_plan)?;
        let is_resume = !self.initial_bitmaps.is_empty();

        let mut tracker = ProgressTracker::new(
            self.session_id,
            RuntimeTransferDirection::Receive,
            self.total_size,
            self.files.len(),
        );
        let file_descs: Vec<FileDesc> = self
            .files
            .iter()
            .map(|f| FileDesc {
                file_id: f.file_id,
                name: f.name.clone(),
                size: f.size,
            })
            .collect();

        let mut bitmaps: HashMap<u32, Vec<u8>> = HashMap::new();
        let mut resume_state = HashMap::new();
        for file in &self.files {
            let total_chunks = calc_total_chunks(file.size);
            let bitmap_len = (total_chunks as usize).div_ceil(8);
            let bitmap = self
                .initial_bitmaps
                .get(&file.file_id)
                .filter(|bm| bm.len() == bitmap_len)
                .cloned()
                .unwrap_or_else(|| vec![0u8; bitmap_len]);
            let completed = count_completed_in_bitmap(&bitmap, total_chunks);
            let bytes = bytes_from_bitmap(&bitmap, file.size, total_chunks);
            if completed > 0 || bytes > 0 {
                resume_state.insert(file.file_id, (completed, bytes));
            }
            bitmaps.insert(file.file_id, bitmap);
        }
        tracker.init_files_with_resume(&file_descs, &resume_state);

        let progress = Arc::new(Mutex::new(tracker));
        let mut sinks: HashMap<u32, FileSinkId> = HashMap::new();
        let mut started_files = HashSet::new();
        let (mut reader, mut writer) = stream.split();
        let (frame_tx, mut frame_rx) = mpsc::channel::<TransferDataFrame>(16);

        let writer_task = async {
            while let Some(frame) = frame_rx.recv().await {
                let is_terminal = matches!(
                    frame,
                    TransferDataFrame::Abort { .. } | TransferDataFrame::Finish { .. }
                );
                write_frame(&mut writer, &frame).await?;
                if is_terminal {
                    break;
                }
            }
            Ok::<(), AppError>(())
        };

        let reader_task = async {
            loop {
                if self.cancel_token.is_cancelled() {
                    return Ok(false);
                }

                match read_frame(&mut reader).await? {
                    Some(TransferDataFrame::BlockData {
                        session_id,
                        epoch: frame_epoch,
                        range,
                        ciphertext,
                    }) if session_id == self.session_id && frame_epoch == epoch => {
                        self.handle_block_data(
                            &frame_tx,
                            &progress,
                            &mut sinks,
                            &mut started_files,
                            &mut bitmaps,
                            is_resume,
                            epoch,
                            range,
                            ciphertext,
                        )
                        .await?;
                    }
                    Some(TransferDataFrame::Finish {
                        session_id,
                        epoch: frame_epoch,
                    }) if session_id == self.session_id && frame_epoch == epoch => {
                        ensure_files_complete(&self.files, &bitmaps)?;
                        self.finish_data_channel(&progress, sinks, bitmaps).await?;
                        frame_tx
                            .send(TransferDataFrame::Finish {
                                session_id: self.session_id,
                                epoch,
                            })
                            .await
                            .map_err(|_| AppError::Transfer("data-channel writer 已关闭".into()))?;
                        return Ok(true);
                    }
                    Some(TransferDataFrame::Abort { reason, .. }) => {
                        return Err(AppError::Transfer(format!("对端中止传输: {reason}")));
                    }
                    Some(other) => {
                        return Err(AppError::Transfer(format!(
                            "接收方收到意外 data frame: {other:?}"
                        )));
                    }
                    None => return Err(AppError::Transfer("data channel 在完成前关闭".into())),
                }
            }
        };

        let (received, _) = tokio::try_join!(reader_task, writer_task)?;
        Ok(received)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "单个 BlockData 处理需要传入运行时上下文"
    )]
    async fn handle_block_data(
        &self,
        frame_tx: &mpsc::Sender<TransferDataFrame>,
        progress: &Arc<Mutex<ProgressTracker>>,
        sinks: &mut HashMap<u32, FileSinkId>,
        started_files: &mut HashSet<u32>,
        bitmaps: &mut HashMap<u32, Vec<u8>>,
        is_resume: bool,
        epoch: i64,
        range: FileRange,
        ciphertext: Vec<u8>,
    ) -> AppResult<()> {
        let file_info = self
            .files
            .iter()
            .find(|file| file.file_id == range.file_id)
            .cloned()
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: {}", range.file_id)))?;
        validate_block_range(&file_info, &range)?;

        let chunk_index = (range.offset / CHUNK_SIZE as u64) as u32;
        let plaintext = self
            .crypto
            .decrypt_chunk(&self.session_id, range.file_id, chunk_index, &ciphertext)
            .map_err(|e| AppError::Transfer(format!("解密失败: {e}")))?;
        if plaintext.len() as u64 != range.length {
            return Err(AppError::Transfer(format!(
                "BlockData 明文长度不匹配: expected={}, actual={}",
                range.length,
                plaintext.len()
            )));
        }

        let sink_id = match sinks.get(&range.file_id).cloned() {
            Some(sink_id) => sink_id,
            None => {
                let metadata = HostFileMetadata {
                    name: file_info.name.clone(),
                    relative_path: file_info.relative_path.clone(),
                    size: file_info.size,
                    modified_at: None,
                    checksum: Some(file_info.checksum.clone()),
                    save_dir: Some(self.save_location.clone()),
                };
                let sink_id = if is_resume {
                    self.file_access.open_or_create_sink(metadata).await
                } else {
                    self.file_access.create_sink(metadata).await
                }?;
                self.created_sinks.lock().await.push(sink_id.clone());
                sinks.insert(range.file_id, sink_id.clone());
                sink_id
            }
        };

        if started_files.insert(range.file_id) {
            let progress_event = {
                let mut p = progress.lock().await;
                p.set_file_transferring(range.file_id);
                p.progress_event()
            };
            if let Some(event) = progress_event {
                let _ = self
                    .event_bus
                    .publish(CoreEvent::TransferProgress { event })
                    .await;
            }
        }

        self.file_access
            .write_sink_chunk(&sink_id, range.offset, plaintext)
            .await?;

        let transferred = {
            let bitmap = bitmaps
                .get_mut(&range.file_id)
                .ok_or_else(|| AppError::Transfer("checkpoint bitmap 不存在".into()))?;
            mark_chunk_completed(bitmap, chunk_index);
            bytes_from_bitmap(bitmap, file_info.size, calc_total_chunks(file_info.size))
        };
        let checkpoint_bitmap = bitmaps
            .get(&range.file_id)
            .cloned()
            .ok_or_else(|| AppError::Transfer("checkpoint bitmap 不存在".into()))?;
        let completed_ranges = ranges_from_bitmap(
            &checkpoint_bitmap,
            file_info.size,
            calc_total_chunks(file_info.size),
        );
        crate::database::ops::update_file_checkpoint_ranges(
            &self.db,
            self.session_id,
            range.file_id as i32,
            checkpoint_bitmap,
            &completed_ranges,
            transferred as i64,
        )
        .await?;

        let progress_event = {
            let mut p = progress.lock().await;
            p.add_bytes(range.length);
            p.update_file_chunk(range.file_id, range.length);
            p.progress_event()
        };
        if let Some(event) = progress_event {
            let _ = self
                .event_bus
                .publish(CoreEvent::TransferProgress { event })
                .await;
        }

        frame_tx
            .send(TransferDataFrame::Ack {
                session_id: self.session_id,
                epoch,
                file_id: range.file_id,
                checkpoint_offset: range.offset + range.length,
            })
            .await
            .map_err(|_| AppError::Transfer("data-channel writer 已关闭".into()))?;

        Ok(())
    }

    async fn finish_data_channel(
        &self,
        progress: &Arc<Mutex<ProgressTracker>>,
        mut sinks: HashMap<u32, FileSinkId>,
        mut bitmaps: HashMap<u32, Vec<u8>>,
    ) -> AppResult<()> {
        for file_info in &self.files {
            let sink_id = match sinks.get(&file_info.file_id).cloned() {
                Some(sink_id) => Some(sink_id),
                None if file_info.size == 0 => {
                    let metadata = HostFileMetadata {
                        name: file_info.name.clone(),
                        relative_path: file_info.relative_path.clone(),
                        size: file_info.size,
                        modified_at: None,
                        checksum: Some(file_info.checksum.clone()),
                        save_dir: Some(self.save_location.clone()),
                    };
                    let sink_id = self.file_access.create_sink(metadata).await?;
                    self.created_sinks.lock().await.push(sink_id.clone());
                    sinks.insert(file_info.file_id, sink_id.clone());
                    Some(sink_id)
                }
                None => None,
            };

            if let Some(sink_id) = sink_id {
                self.file_access.finalize_sink(&sink_id).await?;
                self.remove_created_sink(&sink_id).await;
            }

            let bitmap = bitmaps
                .remove(&file_info.file_id)
                .ok_or_else(|| AppError::Transfer("完成 bitmap 不存在".into()))?;
            crate::database::ops::mark_file_completed(
                &self.db,
                self.session_id,
                file_info.file_id as i32,
                bitmap,
                file_info.size as i64,
            )
            .await?;
        }

        crate::database::ops::mark_session_completed(&self.db, self.session_id).await?;
        let _ = self.coordinator.publish_projection(self.session_id).await;
        self.ensure_inbox_item_after_completion().await;

        let complete_event = progress
            .lock()
            .await
            .complete_event(Some(self.save_location.clone()));
        let _ = self
            .event_bus
            .publish(CoreEvent::TransferCompleted {
                event: complete_event,
            })
            .await;

        Ok(())
    }

    /// 主传输逻辑，返回 true 表示正常完成，false 表示被取消
    async fn run_transfer(self: &Arc<Self>) -> AppResult<bool> {
        let is_resume = !self.initial_bitmaps.is_empty();

        let mut tracker = ProgressTracker::new(
            self.session_id,
            RuntimeTransferDirection::Receive,
            self.total_size,
            self.files.len(),
        );

        let file_descs: Vec<FileDesc> = self
            .files
            .iter()
            .map(|f| FileDesc {
                file_id: f.file_id,
                name: f.name.clone(),
                size: f.size,
            })
            .collect();

        let resume_state: HashMap<u32, (u32, u64)> = file_descs
            .iter()
            .filter_map(|f| {
                let bm = self.initial_bitmaps.get(&f.file_id)?;
                let total = calc_total_chunks(f.size);
                Some((
                    f.file_id,
                    (
                        count_completed_in_bitmap(bm, total),
                        bytes_from_bitmap(bm, f.size, total),
                    ),
                ))
            })
            .collect();
        tracker.init_files_with_resume(&file_descs, &resume_state);

        let progress = Arc::new(Mutex::new(tracker));

        for file_info in &self.files {
            if self.cancel_token.is_cancelled() {
                return Ok(false);
            }

            let total_chunks = calc_total_chunks(file_info.size);

            // 断点续传：通过 file_access 探测文件状态。host 已在调用前保证：
            // - 已最终化的文件不会出现在 self.files 中（host 自己 skip）
            // - 失效 .part 对应的 bitmap 已从 self.initial_bitmaps 中移除
            // 这里只用 bitmap 判断是否完整。
            let initial_bitmap = self.initial_bitmaps.get(&file_info.file_id);
            let effective_bitmap = initial_bitmap;

            let is_fully_complete = effective_bitmap
                .map(|bm| count_completed_in_bitmap(bm, total_chunks) >= total_chunks)
                .unwrap_or(false);

            if is_fully_complete {
                info!(
                    "Skip completed file during resume: {} (file_id={})",
                    file_info.name, file_info.file_id
                );
                continue;
            }

            let progress_event = {
                let mut p = progress.lock().await;
                p.set_file_transferring(file_info.file_id);
                p.progress_event()
            };
            if let Some(event) = progress_event {
                let _ = self
                    .event_bus
                    .publish(CoreEvent::TransferProgress { event })
                    .await;
            }

            let metadata = HostFileMetadata {
                name: file_info.name.clone(),
                relative_path: file_info.relative_path.clone(),
                size: file_info.size,
                modified_at: None,
                checksum: Some(file_info.checksum.clone()),
                save_dir: Some(self.save_location.clone()),
            };
            let sink_id = if is_resume {
                self.file_access.open_or_create_sink(metadata).await
            } else {
                self.file_access.create_sink(metadata).await
            }?;

            self.created_sinks.lock().await.push(sink_id.clone());

            match self
                .pull_file_chunks(
                    file_info,
                    total_chunks,
                    &sink_id,
                    &progress,
                    effective_bitmap,
                )
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    // 暂停/取消时不能 finalize 半截文件。
                    // 这里保留 created_sinks 跟踪：暂停会丢弃 session 从而保留 .part；
                    // 主动取消则由 cancel_receive 后续 cleanup_part_files 清理。
                    return Ok(false);
                }
                Err(e) => {
                    // 不删除 .part 文件——bitmap 已刷写到 DB，保留 .part 以支持断点续传。
                    // .part 文件仅在用户主动取消（cancel_receive）时才清理。
                    self.remove_created_sink(&sink_id).await;
                    self.fail_session(&progress, e.to_string()).await;
                    return Err(e);
                }
            }

            match self.file_access.finalize_sink(&sink_id).await {
                Ok(()) => {
                    self.remove_created_sink(&sink_id).await;
                }
                Err(e) => {
                    self.remove_created_sink(&sink_id).await;
                    // 校验失败意味着 .part 已被删除，必须清除 DB 中的 bitmap，
                    // 否则下次恢复时跳过"已完成"的 chunk 导致数据全零→再次校验失败
                    if let Err(e2) = crate::database::ops::reset_file_checkpoint(
                        &self.db,
                        self.session_id,
                        file_info.file_id as i32,
                    )
                    .await
                    {
                        warn!(
                            "重置文件 checkpoint 失败: file_id={}, {}",
                            file_info.file_id, e2
                        );
                    }
                    let msg = format!(
                        "文件校验失败: {} (file_id={})",
                        file_info.name, file_info.file_id
                    );
                    self.fail_session(&progress, msg).await;
                    return Err(e);
                }
            }

            info!(
                "File verified and saved: {} (file_id={})",
                file_info.name, file_info.file_id
            );

            if let Err(e) = crate::database::ops::mark_file_completed(
                &self.db,
                self.session_id,
                file_info.file_id as i32,
                completed_bitmap(total_chunks),
                file_info.size as i64,
            )
            .await
            {
                warn!("标记文件完成失败: file_id={}, {}", file_info.file_id, e);
            }
        }

        let complete_result = self
            .client
            .send_request(
                self.peer_id,
                AppRequest::Transfer(TransferRequest::Complete {
                    session_id: self.session_id,
                }),
            )
            .await;

        match complete_result {
            Ok(AppResponse::Transfer(TransferResponse::Ack { .. })) => {
                info!(
                    "Transfer complete ack received: session={}",
                    self.session_id
                );
            }
            Ok(other) => {
                warn!("Unexpected complete response: {:?}", other);
            }
            Err(e) => {
                warn!("Failed to send complete message: {}", e);
            }
        }

        if let Err(e) =
            crate::database::ops::mark_session_completed(&self.db, self.session_id).await
        {
            warn!("DB 标记接收完成失败: {}", e);
            let _ = self
                .event_bus
                .publish(CoreEvent::TransferDbError {
                    event: TransferDbErrorEvent {
                        session_id: self.session_id,
                        message: format!("保存完成状态失败: {e}"),
                    },
                })
                .await;
        } else {
            // mark 已双写 phase=terminal/completed；接收方也发 projection，与发送方对称。
            let _ = self.coordinator.publish_projection(self.session_id).await;
            self.ensure_inbox_item_after_completion().await;
        }

        let complete_event = progress
            .lock()
            .await
            .complete_event(Some(self.save_location.clone()));
        let _ = self
            .event_bus
            .publish(CoreEvent::TransferCompleted {
                event: complete_event,
            })
            .await;

        Ok(true)
    }

    /// 并发拉取单个文件的所有分块
    async fn pull_file_chunks(
        self: &Arc<Self>,
        file_info: &FileInfo,
        total_chunks: u32,
        sink_id: &FileSinkId,
        progress: &Arc<Mutex<ProgressTracker>>,
        initial_bitmap: Option<&Vec<u8>>,
    ) -> AppResult<bool> {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHUNKS));
        let has_error = Arc::new(AtomicBool::new(false));
        let first_error: Arc<tokio::sync::Mutex<Option<AppError>>> =
            Arc::new(tokio::sync::Mutex::new(None));

        let bitmap_len = (total_chunks as usize).div_ceil(8);
        // 验证 DB 恢复的 bitmap 长度，不匹配时重置为全零（防止 DB 损坏或 CHUNK_SIZE 变更）
        let valid_bitmap = initial_bitmap.filter(|bm| bm.len() == bitmap_len);
        if let (Some(bm), None) = (initial_bitmap, valid_bitmap) {
            warn!(
                "Bitmap 长度不匹配: expected={}, actual={}, 重置为全零 (file_id={})",
                bitmap_len,
                bm.len(),
                file_info.file_id
            );
        }
        let (initial_completed, initial_bytes) = valid_bitmap
            .map(|bm| {
                (
                    count_completed_in_bitmap(bm, total_chunks),
                    bytes_from_bitmap(bm, file_info.size, total_chunks),
                )
            })
            .unwrap_or((0, 0));
        let initial_bm = valid_bitmap
            .cloned()
            .unwrap_or_else(|| vec![0u8; bitmap_len]);
        let bitmap = Arc::new(tokio::sync::Mutex::new(initial_bm));
        let completed_count = Arc::new(AtomicU32::new(initial_completed));
        let file_transferred = Arc::new(AtomicU64::new(initial_bytes));

        let mut handles = Vec::with_capacity(total_chunks as usize);

        for chunk_index in 0..total_chunks {
            // 跳过已完成的 chunk（断点续传）
            if let Some(bm) = valid_bitmap
                && is_chunk_completed(bm, chunk_index)
            {
                continue;
            }
            // 等待 permit 时同时监听取消，避免取消后仍阻塞在 acquire
            let permit = tokio::select! {
                p = semaphore.clone().acquire_owned() => {
                    p.map_err(|_| AppError::Transfer("Semaphore closed".into()))?
                }
                _ = self.cancel_token.cancelled() => {
                    break;
                }
            };

            let session = self.clone();
            let file_id = file_info.file_id;
            let sink_id = sink_id.clone();
            let progress = progress.clone();
            let has_error = has_error.clone();
            let first_error = first_error.clone();
            let cancel = self.cancel_token.clone();
            let bitmap = bitmap.clone();
            let completed_count = completed_count.clone();
            let file_transferred = file_transferred.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit;

                if cancel.is_cancelled() || has_error.load(Ordering::Relaxed) {
                    return;
                }

                let result = session
                    .pull_single_chunk(file_id, chunk_index, &sink_id)
                    .await;

                match result {
                    Ok(chunk_size) => {
                        let progress_event = {
                            let mut p = progress.lock().await;
                            p.add_bytes(chunk_size as u64);
                            p.update_file_chunk(file_id, chunk_size as u64);
                            p.progress_event()
                        };
                        if let Some(event) = progress_event {
                            let _ = session
                                .event_bus
                                .publish(CoreEvent::TransferProgress { event })
                                .await;
                        }

                        // 单次锁获取：标记 bitmap + 可选 checkpoint 克隆
                        let checkpoint_bm = {
                            let mut bm = bitmap.lock().await;
                            mark_chunk_completed(&mut bm, chunk_index);
                            file_transferred.fetch_add(chunk_size as u64, Ordering::Relaxed);
                            let count = completed_count.fetch_add(1, Ordering::Relaxed) + 1;
                            if count.is_multiple_of(CHECKPOINT_INTERVAL) {
                                Some(bm.clone())
                            } else {
                                None
                            }
                        };

                        if let Some(bm) = checkpoint_bm {
                            let bytes = file_transferred.load(Ordering::Relaxed);
                            if let Err(e) = crate::database::ops::update_file_checkpoint(
                                &session.db,
                                session.session_id,
                                file_id as i32,
                                bm,
                                bytes as i64,
                            )
                            .await
                            {
                                warn!("Bitmap checkpoint 刷写失败: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        if cancel.is_cancelled() {
                            return;
                        }
                        has_error.store(true, Ordering::Relaxed);
                        let mut flag = first_error.lock().await;
                        if flag.is_none() {
                            *flag = Some(e);
                        }
                        cancel.cancel();
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }

        // 无论是取消、错误还是正常完成，都刷写最终 bitmap，确保已完成的 chunk 不丢失
        let has_error = first_error.lock().await.is_some();
        if self.cancel_token.is_cancelled() || has_error {
            let bm = bitmap.lock().await.clone();
            let bytes = file_transferred.load(Ordering::Relaxed);
            if let Err(e) = crate::database::ops::update_file_checkpoint(
                &self.db,
                self.session_id,
                file_info.file_id as i32,
                bm,
                bytes as i64,
            )
            .await
            {
                warn!("bitmap 最终刷写失败: {}", e);
            }
        }

        // 取消 / 中断优先于 error：被 cancel_token 取消的传输（用户取消或对端断连
        // teardown）即使有 in-flight chunk 错误，也算"取消"返回 Ok(false)，不走
        // fail_session（否则中断会被误判为 terminal/failed，盖掉 Interrupted/Cancelled）。
        if self.cancel_token.is_cancelled() {
            return Ok(false);
        }
        if let Some(e) = first_error.lock().await.take() {
            return Err(e);
        }

        Ok(true)
    }

    /// 拉取单个分块（含重试）
    async fn pull_single_chunk(
        &self,
        file_id: u32,
        chunk_index: u32,
        sink_id: &FileSinkId,
    ) -> AppResult<usize> {
        let mut last_error = None;

        for attempt in 0..MAX_CHUNK_RETRIES {
            if self.cancel_token.is_cancelled() {
                return Err(AppError::Transfer("传输已取消".into()));
            }

            if attempt > 0 {
                let delay_ms = RETRY_DELAY_BASE_MS * (1 << (attempt - 1));
                let delay = std::time::Duration::from_millis(delay_ms.min(2000));
                warn!(
                    "Retrying chunk request (attempt {}): file_id={}, chunk_index={}",
                    attempt + 1,
                    file_id,
                    chunk_index
                );
                tokio::time::sleep(delay).await;
            }

            let result = self
                .client
                .send_request(
                    self.peer_id,
                    AppRequest::Transfer(TransferRequest::ChunkRequest {
                        session_id: self.session_id,
                        file_id,
                        chunk_index,
                    }),
                )
                .await;

            match result {
                Ok(AppResponse::Transfer(TransferResponse::Chunk { data, .. })) => {
                    // 解密——失败时纳入重试（数据可能在传输中损坏）
                    let plaintext = match self.crypto.decrypt_chunk(
                        &self.session_id,
                        file_id,
                        chunk_index,
                        &data,
                    ) {
                        Ok(p) => p,
                        Err(e) => {
                            warn!(
                                "解密失败，将重试: file_id={}, chunk={}, {}",
                                file_id, chunk_index, e
                            );
                            last_error = Some(AppError::Transfer(format!(
                                "解密失败: file_id={file_id}, chunk={chunk_index}, {e}"
                            )));
                            continue;
                        }
                    };

                    let chunk_size = plaintext.len();
                    let offset = u64::from(chunk_index) * CHUNK_SIZE as u64;

                    // 通过 FileAccess 写入分块，具体平台写入细节由 host adapter 处理。
                    self.file_access
                        .write_sink_chunk(sink_id, offset, plaintext)
                        .await?;

                    return Ok(chunk_size);
                }
                Ok(AppResponse::Transfer(TransferResponse::ChunkError { error, .. })) => {
                    last_error = Some(AppError::Transfer(format!("发送方报告错误: {error}")));
                }
                Ok(other) => {
                    last_error = Some(AppError::Transfer(format!("意外的响应类型: {other:?}")));
                }
                Err(e) => {
                    last_error = Some(AppError::Transfer(format!("ChunkRequest 失败: {e}")));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::Transfer(format!(
                "分块重试耗尽: file_id={file_id}, chunk={chunk_index}"
            ))
        }))
    }

    /// 发送 Cancel 消息给发送方
    pub async fn send_cancel(&self) {
        let _ = self
            .client
            .send_request(
                self.peer_id,
                AppRequest::Transfer(TransferRequest::Cancel {
                    session_id: self.session_id,
                    reason: "用户取消".into(),
                }),
            )
            .await;
    }

    /// 主动取消
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// 取消并等待后台任务完成（含最终 bitmap 刷写），最多等 5 秒
    pub async fn cancel_and_wait(&self) {
        self.cancel_token.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.wait_finished()).await;
    }

    /// 获取取消令牌
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }

    /// 清理所有已创建但未最终化的临时文件
    pub async fn cleanup_part_files(&self) {
        let sinks = self.created_sinks.lock().await.clone();
        for sink_id in &sinks {
            if let Err(e) = self.file_access.cleanup_sink(sink_id).await {
                warn!(
                    "cleanup receive file sink failed: sink={}, {}",
                    sink_id.0, e
                );
            }
        }
        self.created_sinks.lock().await.clear();
    }

    /// 标记会话失败：写入 DB 失败记录 + 发射失败事件
    async fn fail_session(&self, progress: &Arc<Mutex<ProgressTracker>>, msg: String) {
        let _ = crate::database::ops::mark_session_failed(&self.db, self.session_id, &msg).await;
        // mark 已双写 phase=terminal/failed；发 projection 与发送方对称。
        let _ = self.coordinator.publish_projection(self.session_id).await;
        let event = progress.lock().await.failed_event(msg);
        let _ = self
            .event_bus
            .publish(CoreEvent::TransferFailed { event })
            .await;
    }

    /// 接收完成后创建收件箱索引；失败只作为 DB 附加错误上报，不回滚已完成传输。
    async fn ensure_inbox_item_after_completion(&self) {
        if let Err(e) = crate::database::inbox::ensure_inbox_item_for_completed_receive_session(
            &self.db,
            self.session_id,
        )
        .await
        {
            warn!("创建收件箱条目失败: session={}, {}", self.session_id, e);
            let _ = self
                .event_bus
                .publish(CoreEvent::TransferDbError {
                    event: TransferDbErrorEvent {
                        session_id: self.session_id,
                        message: format!("创建收件箱条目失败: {e}"),
                    },
                })
                .await;
        }
    }

    /// 从跟踪列表中移除指定的 FileSinkId
    async fn remove_created_sink(&self, sink_id: &FileSinkId) {
        let mut sinks = self.created_sinks.lock().await;
        sinks.retain(|id| id != sink_id);
    }
}

// ============ Bitmap 辅助函数 ============

fn ensure_files_complete(files: &[FileInfo], bitmaps: &HashMap<u32, Vec<u8>>) -> AppResult<()> {
    if let Some(range) = first_missing_range(files, bitmaps) {
        return Err(AppError::Transfer(format!(
            "Finish 前仍有未完成数据: file_id={}, offset={}, length={}",
            range.file_id, range.offset, range.length
        )));
    }
    Ok(())
}

fn first_missing_range(files: &[FileInfo], bitmaps: &HashMap<u32, Vec<u8>>) -> Option<FileRange> {
    for file in files {
        if file.size == 0 {
            continue;
        }

        let total_chunks = calc_total_chunks(file.size);
        let bitmap = bitmaps.get(&file.file_id);
        for chunk_index in 0..total_chunks {
            if bitmap
                .map(|bm| !is_chunk_completed(bm, chunk_index))
                .unwrap_or(true)
            {
                return Some(chunk_range(file, chunk_index));
            }
        }
    }
    None
}

fn chunk_range(file: &FileInfo, chunk_index: u32) -> FileRange {
    let offset = chunk_index as u64 * CHUNK_SIZE as u64;
    FileRange {
        file_id: file.file_id,
        offset,
        length: (file.size - offset).min(CHUNK_SIZE as u64),
    }
}

fn validate_block_range(file: &FileInfo, range: &FileRange) -> AppResult<()> {
    let end = range
        .offset
        .checked_add(range.length)
        .ok_or_else(|| AppError::Transfer("BlockData range 溢出".into()))?;
    if end > file.size {
        return Err(AppError::Transfer(format!(
            "BlockData range 超出文件大小: file_id={}, end={}, size={}",
            range.file_id, end, file.size
        )));
    }
    if range.length > CHUNK_SIZE as u64 {
        return Err(AppError::Transfer(format!(
            "BlockData range 超出 chunk 大小: {} > {}",
            range.length, CHUNK_SIZE
        )));
    }
    if file.size > 0 && !range.offset.is_multiple_of(CHUNK_SIZE as u64) {
        return Err(AppError::Transfer(format!(
            "BlockData offset 未按 chunk 对齐: {}",
            range.offset
        )));
    }
    if file.size > 0 && range.length == 0 {
        return Err(AppError::Transfer("非空文件收到空 BlockData".into()));
    }
    Ok(())
}

/// 检查指定 chunk 是否已完成
fn is_chunk_completed(bitmap: &[u8], chunk_index: u32) -> bool {
    let byte_idx = (chunk_index / 8) as usize;
    let bit_idx = chunk_index % 8;
    byte_idx < bitmap.len() && (bitmap[byte_idx] & (1 << bit_idx)) != 0
}

/// 标记指定 chunk 为已完成
fn mark_chunk_completed(bitmap: &mut [u8], chunk_index: u32) {
    let byte_idx = (chunk_index / 8) as usize;
    let bit_idx = chunk_index % 8;
    if byte_idx < bitmap.len() {
        bitmap[byte_idx] |= 1 << bit_idx;
    }
}

/// 构造仅包含有效 chunk 位的完整 bitmap
fn completed_bitmap(total_chunks: u32) -> Vec<u8> {
    let mut bitmap = vec![0u8; (total_chunks as usize).div_ceil(8)];
    for chunk_index in 0..total_chunks {
        mark_chunk_completed(&mut bitmap, chunk_index);
    }
    bitmap
}

/// 统计 bitmap 中已完成的 chunk 数（利用 popcount 加速）
fn count_completed_in_bitmap(bitmap: &[u8], total_chunks: u32) -> u32 {
    let full_bytes = (total_chunks / 8) as usize;
    let remainder_bits = total_chunks % 8;

    let mut count: u32 = bitmap.iter().take(full_bytes).map(|b| b.count_ones()).sum();

    // 尾部不完整字节：仅统计有效位
    if remainder_bits > 0
        && let Some(&last_byte) = bitmap.get(full_bytes)
    {
        let mask = (1u8 << remainder_bits) - 1;
        count += (last_byte & mask).count_ones();
    }

    count
}

/// 根据 bitmap 计算已传输字节数
fn bytes_from_bitmap(bitmap: &[u8], file_size: u64, total_chunks: u32) -> u64 {
    if file_size == 0 || total_chunks == 0 {
        return 0;
    }
    let chunk_size = CHUNK_SIZE as u64;
    let last_chunk_size = match file_size % chunk_size {
        0 => chunk_size,
        r => r,
    };

    let full_chunk_count = count_completed_in_bitmap(bitmap, total_chunks.saturating_sub(1));
    let last_chunk_done = is_chunk_completed(bitmap, total_chunks - 1);

    full_chunk_count as u64 * chunk_size + if last_chunk_done { last_chunk_size } else { 0 }
}

fn ranges_from_bitmap(bitmap: &[u8], file_size: u64, total_chunks: u32) -> Vec<(u64, u64)> {
    let mut ranges = Vec::new();
    let mut current: Option<(u64, u64)> = None;

    for chunk_index in 0..total_chunks {
        if !is_chunk_completed(bitmap, chunk_index) {
            if let Some(range) = current.take() {
                ranges.push(range);
            }
            continue;
        }

        let offset = chunk_index as u64 * CHUNK_SIZE as u64;
        let length = (file_size.saturating_sub(offset)).min(CHUNK_SIZE as u64);
        if length == 0 {
            continue;
        }
        match current.as_mut() {
            Some((_, len)) => *len += length,
            None => current = Some((offset, length)),
        }
    }

    if let Some(range) = current {
        ranges.push(range);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_bitmap_marks_only_valid_chunks() {
        let bitmap = completed_bitmap(10);

        assert_eq!(bitmap.len(), 2);
        assert_eq!(count_completed_in_bitmap(&bitmap, 10), 10);
        assert!(!is_chunk_completed(&bitmap, 10));
    }

    #[test]
    fn bytes_from_bitmap_counts_partial_last_chunk_once() {
        let file_size = CHUNK_SIZE as u64 * 2 + 7;
        let total_chunks = calc_total_chunks(file_size);
        let mut bitmap = vec![0u8; (total_chunks as usize).div_ceil(8)];

        mark_chunk_completed(&mut bitmap, 0);
        mark_chunk_completed(&mut bitmap, 2);

        assert_eq!(
            bytes_from_bitmap(&bitmap, file_size, total_chunks),
            CHUNK_SIZE as u64 + 7
        );
    }

    #[test]
    fn ranges_from_bitmap_merges_contiguous_chunks() {
        let file_size = CHUNK_SIZE as u64 * 3 + 9;
        let total_chunks = calc_total_chunks(file_size);
        let mut bitmap = vec![0u8; (total_chunks as usize).div_ceil(8)];
        mark_chunk_completed(&mut bitmap, 0);
        mark_chunk_completed(&mut bitmap, 1);
        mark_chunk_completed(&mut bitmap, 3);

        assert_eq!(
            ranges_from_bitmap(&bitmap, file_size, total_chunks),
            vec![(0, CHUNK_SIZE as u64 * 2), (CHUNK_SIZE as u64 * 3, 9)]
        );
    }

    #[test]
    fn ensure_files_complete_rejects_missing_nonzero_chunk() {
        let file = FileInfo {
            file_id: 7,
            name: "demo.bin".into(),
            relative_path: "demo.bin".into(),
            size: CHUNK_SIZE as u64 + 1,
            checksum: "hash".into(),
        };
        let mut bitmaps = HashMap::new();
        let mut bitmap = vec![0u8; (calc_total_chunks(file.size) as usize).div_ceil(8)];
        mark_chunk_completed(&mut bitmap, 0);
        bitmaps.insert(file.file_id, bitmap);

        let err = ensure_files_complete(&[file], &bitmaps).unwrap_err();

        assert!(
            err.to_string().contains("Finish 前仍有未完成数据"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn ensure_files_complete_accepts_complete_bitmap() {
        let file = FileInfo {
            file_id: 8,
            name: "complete.bin".into(),
            relative_path: "complete.bin".into(),
            size: CHUNK_SIZE as u64 + 1,
            checksum: "hash".into(),
        };
        let mut bitmaps = HashMap::new();
        bitmaps.insert(file.file_id, completed_bitmap(calc_total_chunks(file.size)));

        assert!(ensure_files_complete(&[file], &bitmaps).is_ok());
    }

    #[test]
    fn ensure_files_complete_treats_zero_size_file_as_complete() {
        let file = FileInfo {
            file_id: 9,
            name: "empty.txt".into(),
            relative_path: "empty.txt".into(),
            size: 0,
            checksum: "hash".into(),
        };

        assert!(ensure_files_complete(&[file], &HashMap::new()).is_ok());
    }
}
