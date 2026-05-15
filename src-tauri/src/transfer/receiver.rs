//! 接收方会话
//!
//! 管理单个接收传输的生命周期：并发拉取分块、解密写入、校验、完成确认。
//! 文件写入通过 [`PartFile`](crate::file_sink::PartFile) 的 OOP 方法完成，
//! 加密使用 [`TransferCrypto`]。
//! 使用 Semaphore 控制并发度（8 并发），CancellationToken 支持取消。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use swarm_p2p_core::libp2p::PeerId;
use swarmdrop_core::host::{FileAccess, FileSinkId, HostFileMetadata};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::file_sink::FileSink;
use crate::file_source::{calc_total_chunks, CHUNK_SIZE};
use crate::protocol::{
    AppNetClient, AppRequest, AppResponse, FileInfo, TransferRequest, TransferResponse,
};
use swarmdrop_core::transfer::crypto::TransferCrypto;
use crate::transfer::progress::{
    FileDesc, ProgressTracker, ProgressTrackerTauriExt, TransferDbErrorEvent, TransferDirection,
};
use crate::{AppError, AppResult};

/// 最大并发拉取数
const MAX_CONCURRENT_CHUNKS: usize = 8;

/// 单个分块最大重试次数
const MAX_CHUNK_RETRIES: u32 = 3;

/// 重试基础延迟
const RETRY_DELAY_BASE_MS: u64 = 500;

/// 每完成多少个 chunk 刷写一次 bitmap checkpoint 到 DB
const CHECKPOINT_INTERVAL: u32 = 10;

fn core_to_app_error(error: swarmdrop_core::AppError) -> AppError {
    AppError::Transfer(error.to_string())
}

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
    /// 文件写入目标（工厂：创建 PartFile + 权限检查）
    sink: FileSink,
    file_access: Arc<dyn FileAccess>,
    /// Tauri AppHandle（用于事件发射和 Android 操作）
    app: AppHandle,
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
        sink: FileSink,
        file_access: Arc<dyn FileAccess>,
        key: &[u8; 32],
        client: AppNetClient,
        app: AppHandle,
        initial_bitmaps: HashMap<u32, Vec<u8>>,
    ) -> Self {
        let (finished_tx, _) = watch::channel(false);
        Self {
            session_id,
            peer_id,
            files,
            total_size,
            sink,
            file_access,
            app,
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

    /// 主传输逻辑，返回 true 表示正常完成，false 表示被取消
    async fn run_transfer(self: &Arc<Self>) -> AppResult<bool> {
        // Android 端在首次写入前请求存储权限
        self.sink.ensure_permission(&self.app).await?;

        let is_resume = !self.initial_bitmaps.is_empty();

        let mut tracker = ProgressTracker::new(
            self.session_id,
            TransferDirection::Receive,
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
                progress
                    .lock()
                    .await
                    .emit_failed(&self.app, "用户取消".into());
                return Ok(false);
            }

            let total_chunks = calc_total_chunks(file_info.size);

            // 断点续传：检查文件是否已被最终化（.part 已重命名为最终文件）
            if is_resume {
                let probe = self
                    .sink
                    .build_part_file(&file_info.relative_path, file_info.size);
                if !probe.final_path.as_os_str().is_empty() && probe.final_path.exists() {
                    info!(
                        "文件已最终化，跳过: {} (file_id={})",
                        file_info.name, file_info.file_id
                    );
                    continue;
                }
            }

            let initial_bitmap = self.initial_bitmaps.get(&file_info.file_id);

            // 断点续传安全检查：.part 文件必须存在且大小正确，否则 bitmap 无效
            // （.part 可能被校验失败删除或磁盘损坏，但 DB bitmap 仍保留）
            let effective_bitmap = if is_resume {
                if let Some(bm) = initial_bitmap {
                    let probe = self
                        .sink
                        .build_part_file(&file_info.relative_path, file_info.size);
                    let part_ok = tokio::fs::metadata(&probe.part_path)
                        .await
                        .map(|m| m.len() == file_info.size)
                        .unwrap_or(false);
                    if part_ok {
                        Some(bm)
                    } else {
                        warn!(
                            ".part 文件不存在或大小不匹配，忽略 bitmap: {} (file_id={})",
                            file_info.name, file_info.file_id
                        );
                        // 同步清除 DB 中的过期 bitmap
                        if let Some(db) = self.app.try_state::<DatabaseConnection>() {
                            let _ = crate::database::ops::reset_file_checkpoint(
                                &db,
                                self.session_id,
                                file_info.file_id as i32,
                            )
                            .await;
                        }
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let is_fully_complete = effective_bitmap
                .map(|bm| count_completed_in_bitmap(bm, total_chunks) >= total_chunks)
                .unwrap_or(false);

            if !is_fully_complete {
                let mut p = progress.lock().await;
                p.set_file_transferring(file_info.file_id);
                p.emit_progress(&self.app);
            }

            let metadata = HostFileMetadata {
                name: file_info.name.clone(),
                relative_path: file_info.relative_path.clone(),
                size: file_info.size,
                modified_at: None,
                checksum: Some(file_info.checksum.clone()),
            };
            let sink_id = if is_resume {
                self.file_access.open_or_create_sink(metadata).await
            } else {
                self.file_access.create_sink(metadata).await
            }
            .map_err(core_to_app_error)?;

            self.created_sinks.lock().await.push(sink_id.clone());

            let pull_result = if is_fully_complete {
                Ok(())
            } else {
                self.pull_file_chunks(
                    file_info,
                    total_chunks,
                    &sink_id,
                    &progress,
                    effective_bitmap,
                )
                .await
            };

            if let Err(e) = pull_result {
                // 不删除 .part 文件——bitmap 已刷写到 DB，保留 .part 以支持断点续传。
                // .part 文件仅在用户主动取消（cancel_receive）时才清理。
                self.remove_created_sink(&sink_id).await;
                self.fail_session(&progress, e.to_string()).await;
                return Err(e);
            }

            match self
                .file_access
                .finalize_sink(&sink_id)
                .await
                .map_err(core_to_app_error)
            {
                Ok(()) => {
                    self.remove_created_sink(&sink_id).await;
                }
                Err(e) => {
                    self.remove_created_sink(&sink_id).await;
                    // 校验失败意味着 .part 已被删除，必须清除 DB 中的 bitmap，
                    // 否则下次恢复时跳过"已完成"的 chunk 导致数据全零→再次校验失败
                    if let Some(db) = self.app.try_state::<DatabaseConnection>() {
                        if let Err(e2) = crate::database::ops::reset_file_checkpoint(
                            &db,
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

        if let Some(db) = self.app.try_state::<DatabaseConnection>() {
            if let Err(e) = crate::database::ops::mark_session_completed(&db, self.session_id).await
            {
                warn!("DB 标记接收完成失败: {}", e);
                let _ = self.app.emit(
                    crate::events::TRANSFER_DB_ERROR,
                    TransferDbErrorEvent {
                        session_id: self.session_id,
                        message: format!("保存完成状态失败: {e}"),
                    },
                );
            }
        }

        progress
            .lock()
            .await
            .emit_complete(&self.app, Some(self.sink.to_save_location()));

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
    ) -> AppResult<()> {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHUNKS));
        let has_error = Arc::new(AtomicBool::new(false));
        let first_error: Arc<tokio::sync::Mutex<Option<AppError>>> =
            Arc::new(tokio::sync::Mutex::new(None));

        let bitmap_len = (total_chunks as usize).div_ceil(8);
        // 验证 DB 恢复的 bitmap 长度，不匹配时重置为全零（防止 DB 损坏或 CHUNK_SIZE 变更）
        let valid_bitmap = initial_bitmap.filter(|bm| bm.len() == bitmap_len);
        if initial_bitmap.is_some() && valid_bitmap.is_none() {
            warn!(
                "Bitmap 长度不匹配: expected={}, actual={}, 重置为全零 (file_id={})",
                bitmap_len,
                initial_bitmap.unwrap().len(),
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
            if let Some(bm) = valid_bitmap {
                if is_chunk_completed(bm, chunk_index) {
                    continue;
                }
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
                        {
                            let mut p = progress.lock().await;
                            p.add_bytes(chunk_size as u64);
                            p.update_file_chunk(file_id, chunk_size as u64);
                            p.emit_progress(&session.app);
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
                            if let Some(db) = session.app.try_state::<DatabaseConnection>() {
                                let bytes = file_transferred.load(Ordering::Relaxed);
                                if let Err(e) = crate::database::ops::update_file_checkpoint(
                                    &db,
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
                    }
                    Err(e) => {
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
            if let Some(db) = self.app.try_state::<DatabaseConnection>() {
                let bm = bitmap.lock().await.clone();
                let bytes = file_transferred.load(Ordering::Relaxed);
                if let Err(e) = crate::database::ops::update_file_checkpoint(
                    &db,
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
        }

        if let Some(e) = first_error.lock().await.take() {
            return Err(e);
        }

        Ok(())
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
                        .await
                        .map_err(core_to_app_error)?;

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
            if let Err(e) = self
                .file_access
                .cleanup_sink(sink_id)
                .await
                .map_err(core_to_app_error)
            {
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
        if let Some(db) = self.app.try_state::<DatabaseConnection>() {
            let _ = crate::database::ops::mark_session_failed(&db, self.session_id, &msg).await;
        }
        let p = progress.lock().await;
        p.emit_failed(&self.app, msg);
    }

    /// 从跟踪列表中移除指定的 FileSinkId
    async fn remove_created_sink(&self, sink_id: &FileSinkId) {
        let mut sinks = self.created_sinks.lock().await;
        sinks.retain(|id| id != sink_id);
    }
}

// ============ Bitmap 辅助函数 ============

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

/// 统计 bitmap 中已完成的 chunk 数（利用 popcount 加速）
fn count_completed_in_bitmap(bitmap: &[u8], total_chunks: u32) -> u32 {
    let full_bytes = (total_chunks / 8) as usize;
    let remainder_bits = total_chunks % 8;

    let mut count: u32 = bitmap.iter().take(full_bytes).map(|b| b.count_ones()).sum();

    // 尾部不完整字节：仅统计有效位
    if remainder_bits > 0 {
        if let Some(&last_byte) = bitmap.get(full_bytes) {
            let mask = (1u8 << remainder_bits) - 1;
            count += (last_byte & mask).count_ones();
        }
    }

    count
}

/// 根据 bitmap 计算已传输字节数
fn bytes_from_bitmap(bitmap: &[u8], file_size: u64, total_chunks: u32) -> u64 {
    if file_size == 0 || total_chunks == 0 {
        return 0;
    }
    let chunk_size = crate::file_source::CHUNK_SIZE as u64;
    let last_chunk_size = match file_size % chunk_size {
        0 => chunk_size,
        r => r,
    };

    let full_chunk_count = count_completed_in_bitmap(bitmap, total_chunks.saturating_sub(1));
    let last_chunk_done = is_chunk_completed(bitmap, total_chunks - 1);

    full_chunk_count as u64 * chunk_size + if last_chunk_done { last_chunk_size } else { 0 }
}
