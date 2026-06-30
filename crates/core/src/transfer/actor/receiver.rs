//! 接收方 actor（ReceiverActor）
//!
//! 管理单个接收传输的生命周期：读取 data-channel 推送的分块、解密写入、校验、最终化。
//! 文件 I/O 全部通过 [`FileAccess`] trait 完成，加密使用 [`TransferCrypto`]。
//! CancellationToken 支持取消。

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use futures::io::AsyncReadExt;
use sea_orm::DatabaseConnection;
use swarm_p2p_core::libp2p::PeerId;
use swarm_p2p_core::libp2p::Stream;
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{
    CoreEvent, CoreSaveLocation, EventBus, FileAccess, FileSinkId, HostFileMetadata,
};
use crate::protocol::{AppNetClient, AppRequest, FileInfo, FileRange, TransferRequest};
use crate::transfer::actor::checkpoint::{
    bytes_from_bitmap, count_completed_in_bitmap, ensure_files_complete, mark_chunk_completed,
    ranges_from_bitmap, validate_block_range,
};
use crate::transfer::coordinator::{ActorReport, CoordinatorInput, TransferCoordinator};
use crate::transfer::progress::{
    FileDesc, ProgressTracker, RuntimeTransferDirection, TransferDbErrorEvent,
};
use crate::transfer::wire::crypto::TransferCrypto;
use crate::transfer::wire::data_frame::{
    TransferDataFrame, manifest_digest, read_frame, write_frame,
};
use crate::transfer::{CHUNK_SIZE, calc_total_chunks};
use crate::{AppError, AppResult};

/// 每完成多少个 chunk 刷写一次 bitmap checkpoint 到 DB
const CHECKPOINT_INTERVAL: u32 = 10;

/// 接收方 actor（ReceiverActor）
pub struct ReceiverActor {
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
    /// 传输完成信号（start_data_channel 的接收循环结束后发送 true）
    finished_tx: watch::Sender<bool>,
}

impl ReceiverActor {
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

        // 接收方在数据面上只顺序读 BlockData 流，收到 Finish 后回写单帧 Finish 确认。
        // 不再用 mpsc writer 桥：读循环天然顺序、仅末尾写一帧，与发送方单 reader/writer 对称
        // （取消由读循环的 select! 直接响应，无需独立 writer task 兜底）。
        loop {
            if self.cancel_token.is_cancelled() {
                return Ok(false);
            }

            // 空闲等待下一帧时也响应取消，避免 cancel 后干等到下一帧或超时。
            let frame = tokio::select! {
                _ = self.cancel_token.cancelled() => return Ok(false),
                frame = read_frame(&mut reader) => frame?,
            };
            match frame {
                Some(TransferDataFrame::BlockData {
                    session_id,
                    epoch: frame_epoch,
                    range,
                    ciphertext,
                }) if session_id == self.session_id && frame_epoch == epoch => {
                    self.handle_block_data(
                        &progress,
                        &mut sinks,
                        &mut started_files,
                        &mut bitmaps,
                        is_resume,
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
                    self.finish_data_channel(epoch, &progress, sinks, bitmaps)
                        .await?;
                    // 回写 Finish 确认：发送方读到它即视为完成（已无逐块 Ack）。
                    write_frame(
                        &mut writer,
                        &TransferDataFrame::Finish {
                            session_id: self.session_id,
                            epoch,
                        },
                    )
                    .await?;
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
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "单个 BlockData 处理需要传入运行时上下文"
    )]
    /// 处理一个入站 BlockData：解密校验 → 落盘 → 节流刷 checkpoint → 发进度。
    /// 各步拆成聚焦的小方法，避免协议/密码学/持久化三层揉在一个 async fn。
    async fn handle_block_data(
        &self,
        progress: &Arc<Mutex<ProgressTracker>>,
        sinks: &mut HashMap<u32, FileSinkId>,
        started_files: &mut HashSet<u32>,
        bitmaps: &mut HashMap<u32, Vec<u8>>,
        is_resume: bool,
        range: FileRange,
        ciphertext: Vec<u8>,
    ) -> AppResult<()> {
        let (file_info, plaintext) = self.decrypt_and_validate(&range, &ciphertext)?;
        let sink_id = self
            .ensure_sink(&file_info, sinks, started_files, progress, is_resume)
            .await?;
        self.persist_chunk(&file_info, &sink_id, &range, plaintext, bitmaps)
            .await?;
        self.emit_chunk_progress(progress, &range).await;
        Ok(())
    }

    /// 找到文件 → 校验 range → 解密 → 校验明文长度，返回 (file_info, plaintext)。
    fn decrypt_and_validate(
        &self,
        range: &FileRange,
        ciphertext: &[u8],
    ) -> AppResult<(FileInfo, Vec<u8>)> {
        let file_info = self
            .files
            .iter()
            .find(|file| file.file_id == range.file_id)
            .cloned()
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: {}", range.file_id)))?;
        validate_block_range(&file_info, range)?;

        let chunk_index = (range.offset / CHUNK_SIZE as u64) as u32;
        let plaintext = self
            .crypto
            .decrypt_chunk(&self.session_id, range.file_id, chunk_index, ciphertext)
            .map_err(|e| AppError::Transfer(format!("解密失败: {e}")))?;
        if plaintext.len() as u64 != range.length {
            return Err(AppError::Transfer(format!(
                "BlockData 明文长度不匹配: expected={}, actual={}",
                range.length,
                plaintext.len()
            )));
        }
        Ok((file_info, plaintext))
    }

    /// 拿到（或首块时创建）该文件的 sink，并在文件首块发"开始传输"进度事件。
    async fn ensure_sink(
        &self,
        file_info: &FileInfo,
        sinks: &mut HashMap<u32, FileSinkId>,
        started_files: &mut HashSet<u32>,
        progress: &Arc<Mutex<ProgressTracker>>,
        is_resume: bool,
    ) -> AppResult<FileSinkId> {
        let sink_id = match sinks.get(&file_info.file_id).cloned() {
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
                sinks.insert(file_info.file_id, sink_id.clone());
                sink_id
            }
        };

        if started_files.insert(file_info.file_id) {
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
        }
        Ok(sink_id)
    }

    /// 落盘明文 → 标记 bitmap → 节流刷 DB checkpoint（每 N 块或末块）。
    async fn persist_chunk(
        &self,
        file_info: &FileInfo,
        sink_id: &FileSinkId,
        range: &FileRange,
        plaintext: Vec<u8>,
        bitmaps: &mut HashMap<u32, Vec<u8>>,
    ) -> AppResult<()> {
        self.file_access
            .write_sink_chunk(sink_id, range.offset, plaintext)
            .await?;

        let chunk_index = (range.offset / CHUNK_SIZE as u64) as u32;
        let total_chunks = calc_total_chunks(file_info.size);
        let (transferred, checkpoint_bitmap) = {
            let bitmap = bitmaps
                .get_mut(&range.file_id)
                .ok_or_else(|| AppError::Transfer("checkpoint bitmap 不存在".into()))?;
            mark_chunk_completed(bitmap, chunk_index);
            let transferred = bytes_from_bitmap(bitmap, file_info.size, total_chunks);
            // 节流：仅每 CHECKPOINT_INTERVAL 块或文件最后一块才刷 DB（含全量 clone + ranges
            // 重算 + 同步写），其余只更新内存 bitmap。中断时最近不足 N 块由续传重拉，完成时
            // mark_file_completed 写最终状态，不丢数据；避免每块 O(n) clone/重算拖垮吞吐。
            let completed = count_completed_in_bitmap(bitmap, total_chunks);
            let checkpoint_bitmap = (completed.is_multiple_of(CHECKPOINT_INTERVAL)
                || completed >= total_chunks)
                .then(|| bitmap.clone());
            (transferred, checkpoint_bitmap)
        };
        if let Some(checkpoint_bitmap) = checkpoint_bitmap {
            let completed_ranges =
                ranges_from_bitmap(&checkpoint_bitmap, file_info.size, total_chunks);
            crate::database::ops::update_file_checkpoint_ranges(
                &self.db,
                self.session_id,
                range.file_id as i32,
                checkpoint_bitmap,
                &completed_ranges,
                transferred as i64,
            )
            .await?;
        }
        Ok(())
    }

    /// 累计已传输字节并发进度事件。
    async fn emit_chunk_progress(&self, progress: &Arc<Mutex<ProgressTracker>>, range: &FileRange) {
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
    }

    async fn finish_data_channel(
        &self,
        epoch: i64,
        progress: &Arc<Mutex<ProgressTracker>>,
        mut sinks: HashMap<u32, FileSinkId>,
        mut bitmaps: HashMap<u32, Vec<u8>>,
    ) -> AppResult<()> {
        for file_info in &self.files {
            // 本会话未收到任何 block 但 bitmap 已完整的文件（上次续传已收完未 finalize、
            // 或 size==0 空文件）也必须 open_or_create + finalize，否则 .part 永不校验落地
            // 却被 mark_file_completed/mark_session_completed→UI 报成功而文件静默丢失。
            // finish 前已 ensure_files_complete，到这里的非空文件 bitmap 必然完整。
            let sink_id = match sinks.get(&file_info.file_id).cloned() {
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
                    let sink_id = self.file_access.open_or_create_sink(metadata).await?;
                    self.created_sinks.lock().await.push(sink_id.clone());
                    sinks.insert(file_info.file_id, sink_id.clone());
                    sink_id
                }
            };

            if let Err(e) = self.file_access.finalize_sink(&sink_id).await {
                self.remove_created_sink(&sink_id).await;
                // 校验失败时 .part 已被删除，但 DB bitmap 仍完整：必须 reset，否则续传/完成
                // 路径会把该文件当作已完成跳过→丢数据。校验失败经 fail_session 转 terminal/failed。
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
                self.fail_session(epoch, progress, msg).await;
                return Err(e);
            }
            self.remove_created_sink(&sink_id).await;

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

        // 终态经状态机：文件级 mark_file_completed 已在上面完成，session 终态由
        // dispatch(Actor{epoch, Completed}) 统一写（带 epoch + terminal 不可逆守卫）。
        // 仅真正转入 completed 才建收件箱索引 + 发完成事件（被取消/旧 epoch 抢先则不发）。
        let transitioned = self
            .coordinator
            .dispatch(
                self.session_id,
                CoordinatorInput::Actor {
                    epoch,
                    report: ActorReport::Completed,
                },
            )
            .await?;
        if transitioned.is_some() {
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
        }

        Ok(())
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

    /// 标记会话失败：终态经状态机 dispatch(Actor{FatalError}) 写 terminal/failed + 发 projection。
    /// 仅真正转入 failed 才发失败事件（被取消/旧 epoch 抢先则不发）。
    async fn fail_session(&self, epoch: i64, progress: &Arc<Mutex<ProgressTracker>>, msg: String) {
        let transitioned = self
            .coordinator
            .dispatch(
                self.session_id,
                CoordinatorInput::Actor {
                    epoch,
                    report: ActorReport::FatalError(msg.clone()),
                },
            )
            .await;
        if matches!(transitioned, Ok(Some(_))) {
            let event = progress.lock().await.failed_event(msg);
            let _ = self
                .event_bus
                .publish(CoreEvent::TransferFailed { event })
                .await;
        }
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
