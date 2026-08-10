//! 接收方 actor（ReceiverActor）
//!
//! 管理单个接收传输的生命周期：读取数据面裸流推送的分块、写入、校验、最终化。
//! 文件 I/O 全部通过 [`FileAccess`] trait 完成。wire v2 已删应用层加密——数据面
//! 直接收明文（见 [`wire`](crate::wire)）。CancellationToken 支持取消。

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use futures::AsyncWriteExt;
use swarmdrop_net::{NodeId, P2pStream};
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::actor::checkpoint::{
    bytes_from_bitmap, count_completed_in_bitmap, ensure_files_complete, file_is_complete,
    mark_chunk_completed, ranges_from_bitmap, validate_block_range,
};
use crate::coordinator::{ActorReport, CoordinatorInput, TransferCoordinator};
use crate::epoch::EpochGuard;
use crate::events::{TransferEvent, TransferEventSink};
use crate::host::{CoreSaveLocation, FileAccess, FileSinkId, HostFileMetadata};
use crate::probe::{
    RECV_CKPT, RECV_LABELS, RECV_REST, RECV_VERIFY, RECV_WAIT, RECV_WRITE, RecvProbe,
};
use crate::progress::{
    FileDesc, ProgressTracker, RuntimeTransferDirection, TransferDbErrorEvent, TransferFailedEvent,
};
use crate::protocol::{FileInfo, FileRange};
use crate::store::TransferStore;
use crate::wire::data_frame::{TransferDataFrame, manifest_digest, read_frame, write_frame};
use crate::{AppError, AppResult};
use crate::{CHUNK_SIZE, calc_total_chunks};

/// 每完成多少个 chunk 刷写一次 bitmap checkpoint 到 DB
const CHECKPOINT_INTERVAL: u32 = 10;

/// 接收方 actor（ReceiverActor）
pub struct ReceiverActor {
    /// 传输会话 ID
    pub session_id: Uuid,
    /// 发送方 NodeId
    pub peer_id: NodeId,
    /// 文件列表
    files: Vec<FileInfo>,
    /// 总大小
    total_size: u64,
    /// 文件访问 trait
    file_access: Arc<dyn FileAccess>,
    /// 事件总线
    events: Arc<dyn TransferEventSink>,
    /// 持久化端口（断点续传 checkpoint / 收件箱条目）
    store: Arc<dyn TransferStore>,
    /// 生命周期协调器（接收方自身 complete/fail 后发 projection，消除收发不对称）
    coordinator: Arc<TransferCoordinator>,
    /// 保存位置（用于完成事件 payload，host 自己定义语义）
    save_location: CoreSaveLocation,
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
        peer_id: NodeId,
        files: Vec<FileInfo>,
        total_size: u64,
        file_access: Arc<dyn FileAccess>,
        events: Arc<dyn TransferEventSink>,
        store: Arc<dyn TransferStore>,
        coordinator: Arc<TransferCoordinator>,
        save_location: CoreSaveLocation,
        initial_bitmaps: HashMap<u32, Vec<u8>>,
    ) -> Self {
        let (finished_tx, _) = watch::channel(false);
        Self {
            session_id,
            peer_id,
            files,
            total_size,
            file_access,
            events,
            store,
            coordinator,
            save_location,
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
            // 对齐在**协商阶段**拒，与发送侧 `resume::validation::validate_fetch_plan` 对称。
            // 不拒的话这个计划会被接受、读循环起来，然后第一个 BlockData 撞
            // `checkpoint::validate_block_range` → Abort → Interrupted → 对端拿同一个
            // Hello 再连一次。一次计划校验一次，好过每块撞一次。
            if file.size > 0 && !crate::is_chunk_aligned_range(range.offset, end, file.size) {
                return Err(AppError::Transfer(format!(
                    "fetch_plan range 未按 chunk 对齐: file_id={}, [{}, {})",
                    range.file_id, range.offset, end
                )));
            }
        }
        Ok(())
    }

    /// 驱动 data-channel 接收**到完成**——在调用方（Router per-stream handler）任务内 await，
    /// **不再 spawn 独立任务**。
    ///
    /// Hello 已由 `TransferManager` 入站路由读取和校验；这里从 BlockData / Finish 开始读。
    ///
    /// **必须与读 Hello 同一个任务**（wasm lost-wakeup 修复）：入站流经 Router handler 任务到达、
    /// Hello 在该任务内读出；若把流 move 给独立 spawn 的任务再读，wasm 单线程下 muxer 已把后续帧的
    /// wake 打给旧 waker（handler 任务——它已不再读此流），新任务首次 poll 注册的新 waker 再无字节
    /// 触发 → 永久 Pending（native 多线程时序掩盖，故 e2e 不显形）。RPC/offer 全程可用正因流从不
    /// 跨任务。故这里改 async、由 handler 任务 await 到终态；registry 清理（`on_finish`）与
    /// `finished_tx` 通知照旧，只是驱动方式从独立 spawn 改为内联 await。
    pub async fn start_data_channel<F>(
        self: Arc<Self>,
        epoch: i64,
        mut stream: P2pStream,
        fetch_plan: Vec<FileRange>,
        on_finish: F,
    ) -> AppResult<()>
    where
        F: FnOnce(&Uuid),
    {
        let outcome = self.run_data_channel(epoch, &mut stream, fetch_plan).await;
        let result = match outcome {
            Ok(true) => {
                info!(
                    "Data-channel receive completed: session={}",
                    self.session_id
                );
                Ok(())
            }
            Ok(false) => {
                info!(
                    "Data-channel receive cancelled: session={}",
                    self.session_id
                );
                Ok(())
            }
            Err(e) => {
                if self.cancel_token.is_cancelled() {
                    info!(
                        "Data-channel receive stopped after cancellation: session={}",
                        self.session_id
                    );
                    Ok(())
                } else {
                    let error = e.to_string();
                    warn!(
                        "Data-channel receive interrupted: session={}, error={}",
                        self.session_id, error
                    );

                    // 数据面错误过去在这里被吞掉并直接 drop 流，发送方只能看到 yamux 的
                    // "connection is closed"。先尽力回写 Abort 并正常关闭写半边，让对端有机会
                    // 读到真正的落盘/校验原因；即使流已坏，本机失败事件仍保留完整诊断。
                    if let Err(abort_error) = write_frame(
                        &mut stream,
                        &TransferDataFrame::Abort {
                            session_id: self.session_id,
                            epoch,
                            reason: error.clone(),
                        },
                    )
                    .await
                    {
                        warn!(
                            "回写 transfer-data Abort 失败: session={}, error={}",
                            self.session_id, abort_error
                        );
                    } else if let Err(close_error) = stream.close().await {
                        warn!(
                            "关闭 transfer-data 写半边失败: session={}, error={}",
                            self.session_id, close_error
                        );
                    }

                    self.emit_best_effort(
                        TransferEvent::TransferFailed {
                            event: TransferFailedEvent {
                                session_id: self.session_id,
                                direction: RuntimeTransferDirection::Receive,
                                error: error.clone(),
                            },
                        },
                        "上报接收失败事件",
                    )
                    .await;

                    if let Err(dispatch_error) = self
                        .coordinator
                        .dispatch(
                            self.session_id,
                            crate::coordinator::CoordinatorInput::Network {
                                epoch,
                                signal: crate::coordinator::NetworkSignal::Interrupted,
                            },
                        )
                        .await
                    {
                        warn!(
                            "dispatch 接收中断失败: session={}, error={}",
                            self.session_id, dispatch_error
                        );
                    }
                    Err(e)
                }
            }
        };

        let _ = self.finished_tx.send(true);
        on_finish(&self.session_id);
        result
    }

    async fn run_data_channel(
        self: &Arc<Self>,
        epoch: i64,
        stream: &mut P2pStream,
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

        // **整流顺序读写，不 split**（wasm 修复，与发送端对称）：读循环天然顺序、仅末尾写一帧
        // Finish 确认，两者不重叠，`split` 本就多余；而 `futures` split 的 BiLock reader half 在
        // wasm 下、数据到达 muxer 后不唤醒读端（native 多线程掩盖）——直接用整条流即修。
        // 逐阶段耗时探针。汇总由 Drop 打，所以下面每条 early return 都会带出数据。
        let mut probe = RecvProbe::new("recv", self.session_id, RECV_LABELS);
        loop {
            if self.cancel_token.is_cancelled() {
                return Ok(false);
            }

            // 空闲等待下一帧时也响应取消，避免 cancel 后干等到下一帧或超时。
            //
            // 这一段是「被链路饿着」的时间。它占大头即说明瓶颈在链路或发送端，
            // **不在接收端的工作量**——这正是把症状 B 的归因从排除法变成正向证据的判据。
            probe.mark();
            let frame = tokio::select! {
                _ = self.cancel_token.cancelled() => return Ok(false),
                frame = read_frame(&mut *stream) => frame?,
            };
            probe.lap(RECV_WAIT);
            match frame {
                Some(TransferDataFrame::BlockData {
                    session_id,
                    epoch: frame_epoch,
                    range,
                    // Approach B：明文在 proof 的 bao 切片里（data 恒空），decode 验签后取回。
                    data: _,
                    proof,
                }) if session_id == self.session_id && EpochGuard::matches(frame_epoch, epoch) => {
                    let length = range.length;
                    self.handle_block_data(
                        &progress,
                        &mut sinks,
                        &mut started_files,
                        &mut bitmaps,
                        is_resume,
                        range,
                        proof,
                        &mut probe,
                    )
                    .await?;
                    probe.block_done(length);
                }
                Some(TransferDataFrame::Finish {
                    session_id,
                    epoch: frame_epoch,
                }) if session_id == self.session_id && EpochGuard::matches(frame_epoch, epoch) => {
                    // 协议级断言：Finish 帧到达时每个文件都必须收齐。它看的是内存 bitmap，
                    // 与「DB bitmap 只在 publish 后才完整」这条持久化纪律无关，两者不冲突。
                    ensure_files_complete(&self.files, &bitmaps)?;
                    self.publish_pending_empty_files(&mut sinks, &mut bitmaps, is_resume)
                        .await?;
                    debug_assert!(
                        sinks.is_empty(),
                        "收齐即发布之后，Finish 时不该还有未发布的 sink: {sinks:?}"
                    );
                    self.finish_data_channel(epoch, &progress).await?;
                    // 回写 Finish 确认：发送方读到它即视为完成（已无逐块 Ack）。
                    write_frame(
                        &mut *stream,
                        &TransferDataFrame::Finish {
                            session_id: self.session_id,
                            epoch,
                        },
                    )
                    .await?;
                    return Ok(true);
                }
                // 流控窗口确认。读到它时，本窗内每一块都已走完
                // `handle_block_data`（验签 → 落盘）——读循环是严格串行的，所以「读到
                // Window」与「窗内块全部消化完」是同一件事，回帧即释放对端下一窗。
                // **不能提前回、也不能异步回**：那会让确认与实际消费速率脱钩，在途量
                // 重新失控，正是本机制要防的东西。
                Some(TransferDataFrame::Window {
                    session_id,
                    epoch: frame_epoch,
                }) if session_id == self.session_id && EpochGuard::matches(frame_epoch, epoch) => {
                    write_frame(
                        &mut *stream,
                        &TransferDataFrame::Window {
                            session_id: self.session_id,
                            epoch,
                        },
                    )
                    .await?;
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
    /// 处理一个入站 BlockData：逐块验签 → 落盘 → 节流刷 checkpoint → 发进度 →
    /// **收齐即发布**。各步拆成聚焦的小方法，避免协议/持久化两层揉在一个 async fn。
    async fn handle_block_data(
        &self,
        progress: &Arc<Mutex<ProgressTracker>>,
        sinks: &mut HashMap<u32, FileSinkId>,
        started_files: &mut HashSet<u32>,
        bitmaps: &mut HashMap<u32, Vec<u8>>,
        is_resume: bool,
        range: FileRange,
        proof: Option<Vec<u8>>,
        probe: &mut RecvProbe,
    ) -> AppResult<()> {
        let (file_info, data) = self.verify_block(&range, proof)?;
        probe.lap(RECV_VERIFY);
        // **已发布的文件不接受任何后续块。** 放行会让 `ensure_sink` 为它新建一条空暂存、
        // 只写进这一块、再发布一次——把用户目录里那个完整文件覆盖成残片。会话末尾统一
        // finalize 的旧实现没有这个窗口（重复块只是重复写同一个 sink），是「收齐即发布」
        // 引入的，必须挡住。
        //
        // 判据直接问位图（`file_is_complete`），不另存一份「已发布」集合：发布成功后
        // `mark_file_completed` 就把位图写成完整，两者恒等价，而多一份状态就多一处会漂。
        //
        // 断流优于静默丢数据——走到这里说明对端与本端对「哪些文件已收齐」的认知已经分叉。
        if file_is_complete(&file_info, bitmaps) {
            return Err(AppError::Transfer(format!(
                "文件已发布后仍收到数据块（协议违规）: file_id={}, offset={}",
                range.file_id, range.offset
            )));
        }
        let sink_id = self
            .ensure_sink(&file_info, sinks, started_files, progress, is_resume)
            .await?;
        let completed_bitmap = self
            .persist_chunk(&file_info, &sink_id, &range, data, bitmaps, probe)
            .await?;
        self.emit_chunk_progress(progress, &range).await;
        if let Some(bitmap) = completed_bitmap {
            self.publish_file(&file_info, sinks, bitmap).await?;
        }
        // 剩下的都归 `rest`：ensure_sink、bitmap 簿记、进度事件、发布。单独拆它们只会
        // 让阶段表变长而不增加判别力——真要拆等这一段先冒头再说。
        probe.lap(RECV_REST);
        Ok(())
    }

    /// 补发布那些**没有数据块可等**的空文件。
    ///
    /// 空文件是「收齐即发布」唯一覆盖不到的情形。首次传输时它还能走通常路径——
    /// `full_fetch_plan` 会给它一条 `length == 0` 的 range，发送端据此发一个空块；
    /// 但**续传的 `build_fetch_plan` 是按字节 range 推导的**（`cursor < file.size`），
    /// 对 `size == 0` 产生不出任何 range。于是「首次传输在空文件的块到达前中断」之后，
    /// 它永远等不到自己的块，而 `ensure_files_complete` 又对 `size == 0` 直接放行——
    /// 会话报完成，文件却从未落地。
    ///
    /// 所以在 Finish 处补一次。**幂等**：已发布的文件位图完整，直接跳过。
    ///
    /// 这是被删掉的那段会话末尾兜底唯一还成立的职责。原来那段还要照顾「上次收完但没
    /// finalize」，那个状态已随 `persist_chunk` 不为末块刷 checkpoint 而消失。
    async fn publish_pending_empty_files(
        &self,
        sinks: &mut HashMap<u32, FileSinkId>,
        bitmaps: &mut HashMap<u32, Vec<u8>>,
        is_resume: bool,
    ) -> AppResult<()> {
        for file_info in &self.files {
            if file_info.size != 0 || file_is_complete(file_info, bitmaps) {
                continue;
            }
            let metadata = HostFileMetadata {
                name: file_info.name.clone(),
                relative_path: file_info.relative_path.clone(),
                size: 0,
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
            sinks.insert(file_info.file_id, sink_id);

            let bitmap = bitmaps
                .get_mut(&file_info.file_id)
                .ok_or_else(|| AppError::Transfer("空文件 checkpoint bitmap 不存在".into()))?;
            mark_chunk_completed(bitmap, 0);
            let bitmap = bitmap.clone();
            self.publish_file(file_info, sinks, bitmap).await?;
        }
        Ok(())
    }

    /// 找到文件 → 校验 range → **逐块验签 proof**，返回 (file_info, 验证过的明文)。
    ///
    /// proof 缺失（`None`）或验证失败 = 协议违规（发送端恒带 proof，v2 内两端同步发布，无渐进
    /// 兼容需求）→ `Err`，调用方按既有 Interrupted 恢复路径断流。验过的明文（decode 输出）
    /// 长度即 `range.length`，逐块验签通过 → 写盘可信 → checkpoint bitmap 本身可信。
    fn verify_block(
        &self,
        range: &FileRange,
        proof: Option<Vec<u8>>,
    ) -> AppResult<(FileInfo, Vec<u8>)> {
        let file_info = self
            .files
            .iter()
            .find(|file| file.file_id == range.file_id)
            .cloned()
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: {}", range.file_id)))?;
        validate_block_range(&file_info, range)?;

        let proof = proof.ok_or_else(|| {
            AppError::Transfer(format!(
                "BlockData 缺少逐块证明（协议违规）: file_id={}, offset={}",
                range.file_id, range.offset
            ))
        })?;
        let root = crate::bao::root_from_checksum(&file_info.checksum)?;
        let data = crate::bao::decode_and_verify(
            &proof,
            root,
            file_info.size,
            range.offset,
            range.length,
        )?;
        Ok((file_info, data))
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
                self.emit_best_effort(
                    TransferEvent::TransferProgress { event },
                    "上报接收文件开始进度",
                )
                .await;
            }
        }
        Ok(sink_id)
    }

    /// 落盘明文 → 标记 bitmap → 节流刷 DB checkpoint。返回**该文件是否已收齐**。
    ///
    /// 节流：仅每 `CHECKPOINT_INTERVAL` 块刷一次 DB（含全量 clone + ranges 重算 + 同步写），
    /// 其余只更新内存 bitmap。中断时最近不足 N 块由续传重拉，避免每块 O(n) clone/重算拖垮吞吐。
    ///
    /// **收齐那一块刻意不刷**——完整 bitmap 只由 publish 成功后的
    /// [`mark_file_completed`](crate::store::SessionStore::mark_file_completed) 写入，
    /// 于是「DB 里 bitmap 完整」⟺「该文件已 publish」。若在这里就把完整 bitmap 落库、
    /// 而随后的 publish 失败，续传时 `first_missing_range` 会跳过该文件，
    /// **再也不会有 block 到达、也就再也不会触发 publish**，文件永久停在 staging，
    /// 却被 `ensure_files_complete` 判为完成。代价是 publish 失败后续传要重拉最多
    /// `CHECKPOINT_INTERVAL - 1` 块，远好过静默丢文件。
    async fn persist_chunk(
        &self,
        file_info: &FileInfo,
        sink_id: &FileSinkId,
        range: &FileRange,
        data: Vec<u8>,
        bitmaps: &mut HashMap<u32, Vec<u8>>,
        probe: &mut RecvProbe,
    ) -> AppResult<Option<Vec<u8>>> {
        self.file_access
            .write_sink_chunk(sink_id, range.offset, data)
            .await?;
        // 写盘单列：症状 B 若真出在接收设备的闪存侧（pSLC 耗尽 / GC），
        // 增长会**只**出现在这一段。
        probe.lap(RECV_WRITE);

        let chunk_index = (range.offset / CHUNK_SIZE as u64) as u32;
        let total_chunks = calc_total_chunks(file_info.size);
        let (transferred, checkpoint_bitmap, completed_bitmap) = {
            let bitmap = bitmaps
                .get_mut(&range.file_id)
                .ok_or_else(|| AppError::Transfer("checkpoint bitmap 不存在".into()))?;
            mark_chunk_completed(bitmap, chunk_index);
            let transferred = bytes_from_bitmap(bitmap, file_info.size, total_chunks);
            let completed = count_completed_in_bitmap(bitmap, total_chunks);
            let file_completed = completed >= total_chunks;
            // 两个 clone 互斥：满窗刷 checkpoint 与收齐发布不会同时发生
            // （末块刻意不刷，见上文）。
            let checkpoint_bitmap = (completed.is_multiple_of(CHECKPOINT_INTERVAL)
                && !file_completed)
                .then(|| bitmap.clone());
            let completed_bitmap = file_completed.then(|| bitmap.clone());
            (transferred, checkpoint_bitmap, completed_bitmap)
        };
        if let Some(checkpoint_bitmap) = checkpoint_bitmap {
            let completed_ranges =
                ranges_from_bitmap(&checkpoint_bitmap, file_info.size, total_chunks);
            self.store
                .update_file_checkpoint_ranges(
                    self.session_id,
                    range.file_id as i32,
                    checkpoint_bitmap,
                    &completed_ranges,
                    transferred as i64,
                )
                .await?;
        }
        // 落库单列：诊断报告实测 Android 上一次 checkpoint 要 11.6 ms（SQLite 未开 WAL，
        // rollback journal + FULL synchronous）。摊到每块约 1.16 ms，是恒定项而非增长项，
        // 但它压低的是稳态吞吐——先看见才好决定要不要动。
        probe.lap(RECV_CKPT);
        Ok(completed_bitmap)
    }

    /// 单个文件收齐 → 立即发布到用户选定的目标位置，并写下完整 checkpoint 与落盘位置。
    ///
    /// 这是「暂存 → 发布」两阶段里的第二阶段，也是文件唯一的落地时刻：host 把 staging 搬到
    /// 目标位置并回报它**最终**在哪（SAF document URI 有独立编码、重名还会被系统改写成
    /// `foo (1).txt`，拼接推导不出来，见 [`FileAccess::finalize_sink`] 的契约）。
    ///
    /// **`finalize_sink` 与 `mark_file_completed` 之间不得插入任何其他 await**：那个窗口里
    /// 进程被杀会留下「staging 已消失、bitmap 却不完整」的状态（design D10 的已知限制）。
    ///
    /// 失败一律 `?` 上抛。publish 不再做校验，失败只意味着「数据是好的，只是搬不过去」
    /// （空间不足 / 权限被撤 / fd 失效），冒泡到
    /// [`start_data_channel`](Self::start_data_channel) 的 Err 分支即是可恢复的 Interrupted。
    /// **不要在这里 reset checkpoint**——数据完好躺在 staging 里，重置只会让对端重传整个文件。
    ///
    /// [`FileAccess::finalize_sink`]: crate::host::FileAccess::finalize_sink
    async fn publish_file(
        &self,
        file_info: &FileInfo,
        sinks: &mut HashMap<u32, FileSinkId>,
        bitmap: Vec<u8>,
    ) -> AppResult<()> {
        let sink_id = sinks.remove(&file_info.file_id).ok_or_else(|| {
            AppError::Transfer(format!("发布时 sink 不存在: file_id={}", file_info.file_id))
        })?;

        let finalized = self.file_access.finalize_sink(&sink_id).await?;
        self.store
            .mark_file_completed(
                self.session_id,
                file_info.file_id as i32,
                bitmap,
                file_info.size as i64,
                finalized.uri,
                finalized.dir,
            )
            .await?;
        self.remove_created_sink(&sink_id).await;
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
            self.emit_best_effort(TransferEvent::TransferProgress { event }, "上报接收块进度")
                .await;
        }
    }

    /// 会话级终态收尾。**不做任何文件级工作**——每个文件在收齐那一刻就已由
    /// [`publish_file`](Self::publish_file) 发布并写库，空文件由
    /// [`publish_pending_empty_files`](Self::publish_pending_empty_files) 兜住。
    async fn finish_data_channel(
        &self,
        epoch: i64,
        progress: &Arc<Mutex<ProgressTracker>>,
    ) -> AppResult<()> {
        // 终态经状态机：文件级 mark_file_completed 已在 publish_file 完成，session 终态由
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
            self.emit_best_effort(
                TransferEvent::TransferCompleted {
                    event: complete_event,
                },
                "上报接收完成事件",
            )
            .await;
        }

        Ok(())
    }

    /// 主动取消
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// 取消并等待后台任务完成（含最终 bitmap 刷写），最多等 5 秒
    pub async fn cancel_and_wait(&self) {
        self.cancel_token.cancel();
        if n0_future::time::timeout(std::time::Duration::from_secs(5), self.wait_finished())
            .await
            .is_err()
        {
            warn!("等待接收任务取消超时: session={}", self.session_id);
        }
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

    /// 接收完成后创建收件箱索引；失败只作为 DB 附加错误上报，不回滚已完成传输。
    ///
    /// 端口返回的 `Option<InboxItemDetail>` 在这条路径上刻意不消费（它是给
    /// `repair_missing_inbox_items_for_completed_receives` 复用 `ensure_*` 用的）：
    /// 接收侧只关心成功与否。
    async fn ensure_inbox_item_after_completion(&self) {
        if let Err(e) = self
            .store
            .ensure_inbox_item_for_completed_receive_session(self.session_id)
            .await
        {
            warn!("创建收件箱条目失败: session={}, {}", self.session_id, e);
            self.emit_best_effort(
                TransferEvent::TransferDbError {
                    event: TransferDbErrorEvent {
                        session_id: self.session_id,
                        message: format!("创建收件箱条目失败: {e}"),
                    },
                },
                "上报收件箱数据库错误",
            )
            .await;
        }
    }

    /// 从跟踪列表中移除指定的 FileSinkId
    async fn remove_created_sink(&self, sink_id: &FileSinkId) {
        let mut sinks = self.created_sinks.lock().await;
        sinks.retain(|id| id != sink_id);
    }
}
