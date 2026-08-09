//! 发送方 actor（SenderActor）
//!
//! 管理单个发送传输的生命周期：经数据面裸流推送文件块、处理 Cancel。
//! 文件读取通过 [`FileAccess`] trait 完成。wire v2 已删应用层加密——Noise/TLS 在途
//! 已加密，数据面直接传明文（见 [`wire`](crate::wire)）。
//! 使用 `Arc<std::sync::Mutex<ProgressTracker>>` 实现并发安全的进度追踪。

use n0_future::time::Instant;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use swarmdrop_net::{NodeId, P2pStream};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::coordinator::{ActorReport, CoordinatorInput, NetworkSignal, TransferCoordinator};
use crate::epoch::EpochGuard;
use crate::events::{TransferEvent, TransferEventSink};
use crate::host::FileAccess;
use crate::manager::PreparedFile;
use crate::progress::{FileDesc, ProgressTracker, RuntimeTransferDirection};
use crate::protocol::{FileInfo, FileRange};
use crate::store::SessionStore;
use crate::wire::data_frame::{
    TransferDataFrame, TransferDataRole, manifest_digest, read_frame, write_frame,
};
use crate::{AppError, AppResult, CHUNK_SIZE, WINDOW_CHUNKS};

/// 流控窗口的簿记。
///
/// 游标要**跨 range 累计**：一个 `FileRange` 可能只够一块（多文件、续传的碎片计划），
/// 按 range 归零等于没有窗口。
///
/// 曾有一个 `limit: Option<u32>` 字段，按协商出的协议名决定要不要流控（退回 v2 的链路上
/// 发窗口帧会被对端判成未知 tag 直接中止）。v2/v3 的注册随 chunk group 变更整体摘除后
/// 只剩一个协议名，流控恒开——字段随之删掉而不是留一个恒等于 `WINDOW_CHUNKS` 的常量位，
/// 免得下一个人以为「上限可按流配置」这件事还成立。
#[derive(Default)]
struct WindowPacer {
    in_window: u32,
}

impl WindowPacer {
    /// 记一块；返回 `true` 表示窗口写满、该同步了（同时归零）。
    fn count_block(&mut self) -> bool {
        self.in_window += 1;
        if self.in_window >= WINDOW_CHUNKS {
            self.in_window = 0;
            return true;
        }
        false
    }
}

/// 发送方 actor（SenderActor）
pub struct SenderActor {
    /// 传输会话 ID
    pub session_id: Uuid,
    /// 对端 NodeId（暂停时需要通知对端）
    pub peer_id: NodeId,
    /// 准备好的文件列表（含文件来源）
    files: Vec<PreparedFile>,
    /// 文件访问 trait（host 实现，桌面=本地路径，RN=expo-fs callback）
    file_access: Arc<dyn FileAccess>,
    /// 事件总线（推送进度等给 host）
    events: Arc<dyn TransferEventSink>,
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
        peer_id: NodeId,
        files: Vec<PreparedFile>,
        file_access: Arc<dyn FileAccess>,
        events: Arc<dyn TransferEventSink>,
    ) -> Self {
        Self::new_inner(
            session_id,
            peer_id,
            files,
            file_access,
            events,
            &HashMap::new(),
        )
    }

    /// 断点续传专用构造函数
    ///
    /// `resume_state` 为每个文件的已完成 chunk 数和已传输字节数（从 DB 读取），
    /// 使 ProgressTracker 从正确的位置开始计数。
    pub fn new_with_resume(
        session_id: Uuid,
        peer_id: NodeId,
        files: Vec<PreparedFile>,
        file_access: Arc<dyn FileAccess>,
        events: Arc<dyn TransferEventSink>,
        resume_state: &HashMap<u32, (u32, u64)>,
    ) -> Self {
        Self::new_inner(
            session_id,
            peer_id,
            files,
            file_access,
            events,
            resume_state,
        )
    }

    fn new_inner(
        session_id: Uuid,
        peer_id: NodeId,
        files: Vec<PreparedFile>,
        file_access: Arc<dyn FileAccess>,
        events: Arc<dyn TransferEventSink>,
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
            file_access,
            events,
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

    /// 将本发送会话绑定到一条数据面裸流，并按 fetch_plan 连续推送数据。
    ///
    /// **整流顺序读写，不 split**（wasm 修复）：`futures::AsyncReadExt::split` 的 BiLock reader
    /// half 在 wasm 下、数据到达 muxer 后不唤醒读端（native 多线程掩盖，浏览器单线程显形，
    /// 现象为「字节到 muxer 但 run_data_channel 读循环不推进」）。改与可用的 RPC 同构——整条流
    /// 顺序 write→read。写完 Hello+全部块+Finish 后，再读对端 Finish 确认。远端中途取消由「写
    /// 出错」或随后读到 Abort 感知；本地取消由每块前的 cancel_token 检查响应。
    ///
    /// 推送**受流控窗口节制**：每 [`WINDOW_CHUNKS`] 块停下等对端确认，见
    /// [`sync_window`](Self::sync_window)。尾窗不必单独同步——紧随其后的 Finish 往返
    /// 本身就是同步点（接收端要收齐全部块才会回 Finish）。
    pub async fn run_data_channel(
        &self,
        epoch: i64,
        mut channel: P2pStream,
        fetch_plan: Vec<FileRange>,
    ) -> AppResult<()> {
        let manifest = self.file_manifest();
        let plan = fetch_plan;

        if let Err(error) = write_frame(
            &mut channel,
            &TransferDataFrame::Hello {
                session_id: self.session_id,
                epoch,
                role: TransferDataRole::Sender,
                manifest_digest: manifest_digest(&manifest),
                fetch_plan: plan.clone(),
            },
        )
        .await
        {
            return Err(self.prefer_remote_abort(&mut channel, epoch, error).await);
        }

        let mut pacer = WindowPacer::default();
        for range in plan {
            if self.cancel_token.is_cancelled() {
                return Err(AppError::Transfer("传输已取消".into()));
            }
            if let Err(error) = self
                .write_range(&mut channel, epoch, range, &mut pacer)
                .await
            {
                return Err(self.prefer_remote_abort(&mut channel, epoch, error).await);
            }
        }

        if let Err(error) = write_frame(
            &mut channel,
            &TransferDataFrame::Finish {
                session_id: self.session_id,
                epoch,
            },
        )
        .await
        {
            return Err(self.prefer_remote_abort(&mut channel, epoch, error).await);
        }

        // 接收方收完并 finalize 后回一帧 Finish 作为完成确认（已无逐块 Ack），读到它即完成。
        // 空闲等待时响应取消，避免 cancel 后干等到对端 Finish 或超时。
        let frame = tokio::select! {
            _ = self.cancel_token.cancelled() => {
                return Err(AppError::Transfer("传输已取消".into()));
            }
            frame = read_frame(&mut channel) => frame?,
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
    }

    /// 接收方在本地落盘/校验失败时会先回写 Abort 再关流。发送端往往正处在下一块
    /// `write_frame`，底层先返回的只是 yamux "connection is closed"；此时短暂读取反向
    /// 数据，可把已经到达的 Abort 原因提升为最终错误，避免 UI 只展示传输层噪声。
    async fn prefer_remote_abort(
        &self,
        channel: &mut P2pStream,
        epoch: i64,
        original: AppError,
    ) -> AppError {
        let is_stream_io = matches!(
            &original,
            AppError::Transfer(message) if message.starts_with("transfer-data IO 错误")
        );
        if !is_stream_io {
            return original;
        }

        match n0_future::time::timeout(std::time::Duration::from_millis(750), read_frame(channel))
            .await
        {
            Ok(Ok(Some(TransferDataFrame::Abort {
                session_id,
                epoch: abort_epoch,
                reason,
            }))) if session_id == self.session_id && EpochGuard::matches(abort_epoch, epoch) => {
                AppError::Transfer(format!("对端中止传输: {reason}"))
            }
            _ => original,
        }
    }

    /// 发送数据面正常结束的终态副作用（与接收方 `finish_data_channel` 对称）。
    ///
    /// session 终态经状态机 `dispatch(Actor{epoch, Completed})`，享受 epoch + terminal
    /// 不可逆守卫（旧 epoch / 已取消的会话不被覆盖）；仅真正转入 completed 才发完成事件。
    ///
    /// **落进度与 [`on_interrupted`](Self::on_interrupted) 对称，不是可选步骤。**
    /// 发送侧的进度只活在内存 `ProgressTracker` 里——接收侧有 `persist_chunk` 逐块增量
    /// 落库，发送侧没有对应物，只在几个终态路径批量落一次。完成路径此前漏了这一步，于是
    /// `transfer_file.transferred_bytes` 永远停在 0，而 projection 的 `transferredBytes`
    /// 是**文件级 SUM**（见 `store::projection_of`），结果传完的会话在发送方 UI 上显示成
    /// 「已完成 0 B / 500 MB 0%」，接收方同一条却是 100%（2026-08-04 实测，三条会话无一例外）。
    pub async fn on_completed(
        &self,
        epoch: i64,
        coordinator: &TransferCoordinator,
        store: &dyn SessionStore,
    ) {
        let progress = self.get_file_progress();
        if let Err(error) = store
            .save_sender_file_progress(self.session_id, &progress)
            .await
        {
            warn!(
                "保存发送方完成进度失败: session={}, error={}",
                self.session_id, error
            );
        }
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
                // 不再手搓 TransferCompleteEvent。
                let event = match self.progress.lock() {
                    Ok(progress) => Some(progress.complete_event(None)),
                    Err(_) => {
                        warn!(
                            "读取发送完成进度失败（锁中毒）: session={}",
                            self.session_id
                        );
                        None
                    }
                };
                if let Some(event) = event {
                    self.emit_best_effort(
                        TransferEvent::TransferCompleted { event },
                        "上报发送完成事件",
                    )
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
        store: &dyn SessionStore,
    ) {
        let progress = self.get_file_progress();
        if let Err(error) = store
            .save_sender_file_progress(self.session_id, &progress)
            .await
        {
            warn!(
                "保存发送方中断进度失败: session={}, error={}",
                self.session_id, error
            );
        }
        if let Err(error) = coordinator
            .dispatch(
                self.session_id,
                CoordinatorInput::Network {
                    epoch,
                    signal: NetworkSignal::Interrupted,
                },
            )
            .await
        {
            warn!(
                "dispatch 发送中断失败: session={}, error={}",
                self.session_id, error
            );
        }
    }

    fn file_manifest(&self) -> Vec<FileInfo> {
        self.files.iter().map(FileInfo::from).collect()
    }

    /// 一个流控窗口写满：发一帧 Window 并等对端回同款，期间响应本地取消。
    ///
    /// 这是数据面唯一的端到端流控（缘由见
    /// [`TransferDataFrame::Window`](crate::wire::data_frame::TransferDataFrame::Window)）。
    ///
    /// **停等，不是滑动窗口。** 滑动要在写的同时读确认，而数据面是刻意的整流顺序读写
    /// （不 split，理由见 [`run_data_channel`](Self::run_data_channel)）。代价是每窗一个
    /// RTT：4 MiB 一窗，20 MB 只停 5 次，相对传输本身可忽略——稳态吞吐由接收端的验签
    /// 落盘速率决定，窗口不是瓶颈。换来的是**自证的正确性**：确认与窗序天然一一对应，
    /// 无需推理乱序 ack。
    async fn sync_window<S>(&self, stream: &mut S, epoch: i64) -> AppResult<()>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        write_frame(
            stream,
            &TransferDataFrame::Window {
                session_id: self.session_id,
                epoch,
            },
        )
        .await?;

        let frame = tokio::select! {
            _ = self.cancel_token.cancelled() => {
                return Err(AppError::Transfer("传输已取消".into()));
            }
            frame = read_frame(stream) => frame?,
        };
        match frame {
            Some(TransferDataFrame::Window {
                session_id,
                epoch: ack_epoch,
            }) if session_id == self.session_id && EpochGuard::matches(ack_epoch, epoch) => Ok(()),
            Some(TransferDataFrame::Abort { reason, .. }) => {
                Err(AppError::Transfer(format!("对端中止传输: {reason}")))
            }
            Some(other) => Err(AppError::Transfer(format!(
                "发送方等待窗口确认时收到意外 data frame: {other:?}"
            ))),
            None => Err(AppError::Transfer("data channel 在窗口确认前关闭".into())),
        }
    }

    /// 写一块并推进窗口游标，满窗就地同步。
    ///
    /// 存在的理由只有一个：`write_range` 有两条写块路径（空文件的单空块 / 常规循环），
    /// 窗口簿记漏在任何一条上，都会让在途量悄悄越过上限。
    async fn write_block_paced<S>(
        &self,
        stream: &mut S,
        epoch: i64,
        file: &PreparedFile,
        offset: u64,
        length: usize,
        pacer: &mut WindowPacer,
    ) -> AppResult<()>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        self.write_block(stream, epoch, file, offset, length)
            .await?;
        if pacer.count_block() {
            self.sync_window(stream, epoch).await?;
        }
        Ok(())
    }

    async fn write_range<S>(
        &self,
        stream: &mut S,
        epoch: i64,
        range: FileRange,
        pacer: &mut WindowPacer,
    ) -> AppResult<()>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
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
            self.write_block_paced(stream, epoch, file, 0, 0, pacer)
                .await?;
            return Ok(());
        }

        let mut offset = range.offset;
        while offset < end {
            if self.cancel_token.is_cancelled() {
                return Err(AppError::Transfer("传输已取消".into()));
            }
            let len = ((end - offset) as usize).min(CHUNK_SIZE);
            self.write_block_paced(stream, epoch, file, offset, len, pacer)
                .await?;
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
        // range 已按 file.size clamp，EOF 短读在此不可能——长度不符只能是宿主违约
        // 或文件在传输中被外部改动。静默发出缩短的块会让外层循环仍按请求 len 推进
        // offset，留下永不补发的 gap（接收端 checkpoint 永远收不齐），必须响错。
        if plaintext.len() != length {
            return Err(AppError::Transfer(format!(
                "read_source_chunk 返回长度异常: 请求 {length}B@{offset}，得到 {}B (file_id={})",
                plaintext.len(),
                file.file_id
            )));
        }
        let plaintext_len = plaintext.len() as u64;

        // 逐块验签（Approach B）：proof 携带该 range 的完整 bao 切片（含叶子），data 置空——
        // 叶子只在 proof 出现一次，无 2x 冗余。接收端 decode 必然验签，验过即写盘。
        let root = crate::bao::root_from_checksum(&file.checksum)?;
        let proof = crate::bao::encode_proof(&file.outboard, root, file.size, offset, &plaintext)?;

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
                data: Vec::new(),
                proof: Some(proof),
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
            self.emit_best_effort(TransferEvent::TransferProgress { event }, "上报发送块进度")
                .await;
        }

        Ok(())
    }

    /// UI 事件投递失败不改变传输状态，但必须留诊断，避免状态已落库而界面无反馈。
    async fn emit_best_effort(&self, event: TransferEvent, operation: &'static str) {
        if let Err(error) = self.events.emit(event).await {
            warn!(
                session = %self.session_id,
                %error,
                operation,
                "传输事件投递失败"
            );
        }
    }

    /// 返回自上次活动以来的空闲时间（毫秒）
    pub fn idle_ms(&self) -> u64 {
        let elapsed = self.created_at.elapsed().as_millis() as u64;
        let last = self.last_activity_ms.load(Ordering::Relaxed);
        elapsed.saturating_sub(last)
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;

    use futures::io::Cursor as IoCursor;
    use swarmdrop_net::SecretKey;

    use super::*;
    use crate::WINDOW_CHUNKS;
    use crate::host::{FileSinkId, FileSourceId, FinalizedSink, HostFileMetadata};

    /// 收下所有写入、**读侧永远 Pending** 的对端替身：模拟「不回窗口确认的接收方」。
    #[derive(Clone, Default)]
    struct StalledPeer(Arc<Mutex<Vec<u8>>>);

    impl futures::io::AsyncWrite for StalledPeer {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl futures::io::AsyncRead for StalledPeer {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }
    }

    /// 按偏移切一份内存里的源数据；事件全丢弃。
    struct MemorySource(Vec<u8>);

    #[async_trait::async_trait]
    impl FileAccess for MemorySource {
        async fn read_source_chunk(
            &self,
            _source: &FileSourceId,
            offset: u64,
            length: usize,
        ) -> AppResult<Vec<u8>> {
            let start = offset as usize;
            Ok(self.0[start..start + length].to_vec())
        }
        async fn source_metadata(&self, _source: &FileSourceId) -> AppResult<HostFileMetadata> {
            unreachable!("本组测试不读元数据")
        }
        async fn delete_finalized_file(&self, _uri: &str) -> AppResult<()> {
            unreachable!("本组测试不删文件")
        }
        async fn create_sink(&self, _metadata: HostFileMetadata) -> AppResult<FileSinkId> {
            unreachable!("本组测试不写文件")
        }
        async fn open_or_create_sink(&self, _metadata: HostFileMetadata) -> AppResult<FileSinkId> {
            unreachable!("本组测试不写文件")
        }
        async fn write_sink_chunk(
            &self,
            _sink: &FileSinkId,
            _offset: u64,
            _data: Vec<u8>,
        ) -> AppResult<()> {
            unreachable!("本组测试不写文件")
        }
        async fn finalize_sink(&self, _sink: &FileSinkId) -> AppResult<FinalizedSink> {
            unreachable!("本组测试不落盘")
        }
    }

    #[async_trait::async_trait]
    impl TransferEventSink for MemorySource {
        async fn emit(&self, _event: TransferEvent) -> AppResult<()> {
            Ok(())
        }
    }

    /// 读侧不断供给窗口确认的对端替身：模拟「消化得过来的接收方」。
    ///
    /// 供给到末尾就回到开头，于是任意多个窗口都能被确认——`write_range` 只写不读
    /// 别的帧，不会误读到内容。
    #[derive(Clone)]
    struct AckingPeer {
        written: Arc<Mutex<Vec<u8>>>,
        ack: Arc<Vec<u8>>,
        pos: usize,
    }

    impl futures::io::AsyncWrite for AckingPeer {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.written.lock().unwrap().extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl futures::io::AsyncRead for AckingPeer {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            if this.pos >= this.ack.len() {
                this.pos = 0;
            }
            let n = buf.len().min(this.ack.len() - this.pos);
            buf[..n].copy_from_slice(&this.ack[this.pos..this.pos + n]);
            this.pos += n;
            Poll::Ready(Ok(n))
        }
    }

    /// 数据面流控的另一半：**收到确认后必须继续推完**。
    ///
    /// 与 [`sender_stops_after_one_window_until_peer_acks`] 配对——只有那一条时，把
    /// `sync_window` 写成「永远返回 Pending」也能让它通过，而那会把每一次传输都卡死在
    /// 第一窗。
    #[tokio::test]
    async fn sender_resumes_after_each_window_ack() {
        let session_id = Uuid::new_v4();
        let epoch = 3;
        // 三窗零头：确保确认路径被走满三次，而不是只在收尾时碰巧过一次。
        let total_chunks = crate::WINDOW_CHUNKS * 3 + 1;
        let data = vec![9u8; total_chunks as usize * CHUNK_SIZE];
        let (root, outboard) = crate::bao::build_outboard(&data);

        let mut ack = Vec::new();
        write_frame(&mut ack, &TransferDataFrame::Window { session_id, epoch })
            .await
            .unwrap();

        let host = Arc::new(MemorySource(data.clone()));
        let actor = SenderActor::new(
            session_id,
            SecretKey::generate().node_id(),
            vec![PreparedFile {
                file_id: 0,
                name: "big.bin".into(),
                relative_path: "big.bin".into(),
                source_id: FileSourceId("mem".into()),
                size: data.len() as u64,
                checksum: root.to_hex().to_string(),
                outboard,
            }],
            host.clone(),
            host,
        );

        let mut peer = AckingPeer {
            written: Arc::new(Mutex::new(Vec::new())),
            ack: Arc::new(ack),
            pos: 0,
        };
        let mut pacer = WindowPacer::default();
        let range = FileRange {
            file_id: 0,
            offset: 0,
            length: data.len() as u64,
        };
        n0_future::time::timeout(
            Duration::from_secs(20),
            actor.write_range(&mut peer, epoch, range, &mut pacer),
        )
        .await
        .expect("对端持续确认时不该卡住")
        .expect("推送应当成功");

        let written = peer.written.lock().unwrap().clone();
        let mut cursor = IoCursor::new(written);
        let (mut blocks, mut windows) = (0u32, 0u32);
        while let Some(frame) = read_frame(&mut cursor).await.unwrap() {
            match frame {
                TransferDataFrame::BlockData { .. } => blocks += 1,
                TransferDataFrame::Window { .. } => windows += 1,
                other => panic!("不该出现 {other:?}"),
            }
        }
        assert_eq!(blocks, total_chunks, "所有块都应推出去");
        assert_eq!(windows, 3, "满窗三次，确认三次");
    }

    /// 数据面流控的护栏：**满一窗必须停下等确认**。
    ///
    /// 去掉窗口后传输在 native↔native 之间照样跑得通（那边 yamux/QUIC 顶着），只有
    /// 浏览器接收端会在几 MiB 处被撑爆——那是跑不进 CI 的失效模式，所以这条断言是它
    /// 唯一的机器守卫。改 `WINDOW_CHUNKS` 不会让它红（它按常量算期望值），删掉窗口会。
    #[tokio::test]
    async fn sender_stops_after_one_window_until_peer_acks() {
        let total_chunks = WINDOW_CHUNKS + 1;
        let data = vec![7u8; total_chunks as usize * CHUNK_SIZE];
        let (root, outboard) = crate::bao::build_outboard(&data);

        let host = Arc::new(MemorySource(data.clone()));
        let actor = SenderActor::new(
            Uuid::new_v4(),
            SecretKey::generate().node_id(),
            vec![PreparedFile {
                file_id: 0,
                name: "big.bin".into(),
                relative_path: "big.bin".into(),
                source_id: FileSourceId("mem".into()),
                size: data.len() as u64,
                checksum: root.to_hex().to_string(),
                outboard,
            }],
            host.clone(),
            host,
        );

        let mut peer = StalledPeer::default();
        let mut pacer = WindowPacer::default();
        let range = FileRange {
            file_id: 0,
            offset: 0,
            length: data.len() as u64,
        };
        let outcome = n0_future::time::timeout(
            Duration::from_millis(500),
            actor.write_range(&mut peer, 1, range, &mut pacer),
        )
        .await;
        assert!(
            outcome.is_err(),
            "对端不回确认时，发送端必须停在窗口边界，而不是把整个文件推出去"
        );

        let written = peer.0.lock().unwrap().clone();
        let mut cursor = IoCursor::new(written);
        let (mut blocks, mut windows) = (0u32, 0u32);
        while let Some(frame) = read_frame(&mut cursor).await.unwrap() {
            match frame {
                TransferDataFrame::BlockData { .. } => blocks += 1,
                TransferDataFrame::Window { .. } => windows += 1,
                other => panic!("窗口边界前不该出现 {other:?}"),
            }
        }
        assert_eq!(blocks, WINDOW_CHUNKS, "停下前恰好推满一窗");
        assert_eq!(windows, 1, "并且已经发出窗口同步帧");
    }
}
