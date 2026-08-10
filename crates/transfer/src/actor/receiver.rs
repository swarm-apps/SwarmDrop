//! 接收方 actor（ReceiverActor）
//!
//! 管理单个接收传输的生命周期：读取数据面裸流推送的分块、写入、校验、最终化。
//! 文件 I/O 全部通过 [`FileAccess`] trait 完成。wire v2 已删应用层加密——数据面
//! 直接收明文（见 [`wire`](crate::wire)）。CancellationToken 支持取消。

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use futures::channel::mpsc;
use futures::{AsyncWriteExt, SinkExt as _, StreamExt as _};
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
    DIGEST_CKPT, DIGEST_LABELS, DIGEST_PUBLISH, DIGEST_QUEUE, DIGEST_REST, DIGEST_VERIFY,
    DIGEST_WRITE, DigestProbe, FRAME_ENQUEUE, FRAME_LABELS, FRAME_WAIT, FrameProbe,
};
use crate::progress::{
    FileDesc, FilePublishEvent, FilePublishPhase, ProgressTracker, RuntimeTransferDirection,
    TransferDbErrorEvent, TransferFailedEvent,
};
use crate::protocol::{FileInfo, FileRange};
use crate::store::TransferStore;
use crate::wire::data_frame::{TransferDataFrame, manifest_digest, read_frame, write_frame};
use crate::{AppError, AppResult};
use crate::{CHUNK_SIZE, WINDOW_CHUNKS, calc_total_chunks};

/// 每完成多少个 chunk 刷写一次 bitmap checkpoint 到 DB
const CHECKPOINT_INTERVAL: u32 = 10;

/// 收帧 → 消化之间那条有界队列的容量，**恰好一个流控窗口**。
///
/// 这个数不是调出来的，它由「收帧循环读到 `Window` 时不该被队列顶住」这条要求唯一确定：
/// 对端一窗最多推 [`WINDOW_CHUNKS`] 块，队列装得下整窗，收帧循环才能立即回 `Window`
/// 放行下一窗，流水线才真正满起来。再大只是多囤字节，不会更快。
///
/// # 背压仍然成立，而且**不依赖传输层**
///
/// 队列满 → 收帧循环挂在 `send` 上 → 读不到那一帧 `Window` → 不回确认 → 对端停在窗口
/// 边界。整条链是应用层自己闭合的，这点很关键：浏览器的 `RTCDataChannel` **没有接收侧
/// 背压**（`onmessage` 一触发就释放 SCTP 接收缓冲，接收窗口永不收缩，见 `webrtc-p2p` 的
/// `DEFAULT_MAX_READ_BUFFER` 那段），所以「不读流」在 Web 端根本回压不了对端。指望传输层
/// 的设计在这里会静默失效，指望 `Window` 的不会。
///
/// # 读缓冲峰值没有变
///
/// 容易误判成「在途量翻倍了」。实际分两处、各自有界：对端拿到一次 `Window` 确认后最多
/// 再推一窗，**传输层读缓冲的峰值仍是 16 块 = 4 MiB，与流水线化之前相同**；新增的 4 MiB
/// 是本进程内已读出、待消化的队列，不占读缓冲。故 16 MiB 上限的 4× 余量原样保留。
const DIGEST_QUEUE_CHUNKS: usize = WINDOW_CHUNKS as usize;

/// 已收帧、等待消化的一块。
///
/// 只带 `proof`：wire v2 起明文就在 proof 的 bao 切片里，`BlockData.data` 恒空。
struct PendingBlock {
    range: FileRange,
    proof: Option<Vec<u8>>,
}

/// 收帧循环的终止原因。**只有这两种是正常终止**，其余一律 `Err`。
enum FrameLoopEnd {
    /// 本地取消。
    Cancelled,
    /// 收到对端 `Finish`。**不代表数据已消化完**——队列里可能还压着块，
    /// 所以 `Finish` 确认必须等消化循环也返回之后才写。
    Finished,
}

/// 消化循环独占、并在结束时交还的状态。
///
/// 这两张表**只有消化循环碰得到**，收帧循环连引用都拿不到——这是拆成两条路径后
/// 唯一需要的同步纪律，靠所有权表达比靠约定可靠。
struct Digested {
    sinks: HashMap<u32, FileSinkId>,
    bitmaps: HashMap<u32, Vec<u8>>,
}

/// 小于这个尺寸的文件不广播发布阶段事件。
///
/// 三端都只在发布持续超过 `PUBLISH_VISIBLE_AFTER_MS`（300ms）后才把「正在保存」揭示出来，
/// 而 1 MiB 的拷贝在最慢的 SAF 目标上也只有几十毫秒——这条事件必然被丢弃。取这个值是因为
/// 它同时兜住了「几万个小文件」这个真实形态：那种会话里每文件两条事件会白白多推几万条
/// IPC 消息，而发射点就 await 在收块热路径上。
const PUBLISH_ANNOUNCE_MIN_BYTES: u64 = 1 << 20;

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

        // 收帧与消化并发跑，中间隔一条有界队列（[`DIGEST_QUEUE_CHUNKS`]）。
        //
        // # 为什么要拆
        //
        // 拆之前两件事在同一个循环里严格串行：读一帧 → 验签落盘 → 再读下一帧。于是对端
        // 在等我们消化、我们在等对端开发，两条路径**永远不重叠**——2026-08-10 的三端实测
        // 里，发送侧 `ack` 占 88% 与接收侧 `wait` 占 67% 同时成立，正是这个形态的指纹
        // （真要重叠，不可能双方等待都占大头）。把三段耗时拆开看，接收端处理占了整场
        // 32.8%，而它本可以完全藏在网络时间背后。
        //
        // # 为什么不是 `split`
        //
        // 这里**依然没有 split 任何流**：`stream` 整条归收帧循环独占，消化循环碰都碰不到它
        // （它只认队列和存储）。`futures` split 的 BiLock reader half 在 wasm 下数据到达
        // muxer 后不唤醒读端（native 多线程掩盖，浏览器单线程显形），那条坑与这次改动无关，
        // 也不会被它勾出来——这正是选「一条流 + 一条队列」而不是「split 成读写两半」的理由。
        //
        // # 并发而非并行
        //
        // `join` 在**同一个任务**里驱动两条 future 交错前进，不 spawn、不要求 `Send`。
        // wasm 单线程一样成立：消化循环每个 await 点（写盘、落库）都会让出，收帧循环随即推进。
        //
        // # 为什么是 `join` 而不是 `try_join`
        //
        // `try_join` 会在第一个 `Err` 上短路，**把另一条 future 就地 drop**——那会在
        // `publish_file` 的 await 点中间取消消化循环。Android 的 SAF 发布是一次几十秒的
        // 全量字节拷贝，而宿主那侧的拷贝 promise 取消不掉：文件照样落到用户目录，
        // `mark_file_completed` 却再也不会执行。于是 DB bitmap 停在不完整状态，恢复时整个
        // 文件重传并**再发布一次**，用户目录里多出一个 `foo (1).ext`。这正是
        // [`publish_file`](Self::publish_file) 文档里声明「只有强杀进程才能到达」的那个状态。
        //
        // `join` 等两条都收敛：收帧端出错 → `block_tx` 随之 drop → 消化端把队列里剩下的块
        // 处理完再返回。多花的时间以队列深度为上限（≤ 一个窗口），换掉一整类恢复期的重复发布。
        let (block_tx, block_rx) = mpsc::channel(DIGEST_QUEUE_CHUNKS);
        let (frame_result, digest_result) = futures::future::join(
            self.run_frame_loop(&mut *stream, epoch, block_tx),
            self.run_digest_loop(block_rx, &progress, bitmaps, is_resume),
        )
        .await;

        // **消化端的错误先抛**：它携带的是真实失败原因（验签失败、写盘失败…），而收帧端
        // 此时多半只会报一句次生的「消化端已退出」。反过来顺序就会把归因盖掉。
        let digested = digest_result?;
        let end = frame_result?;

        let Digested {
            mut sinks,
            mut bitmaps,
        } = digested;
        match end {
            FrameLoopEnd::Cancelled => Ok(false),
            FrameLoopEnd::Finished => {
                // 协议级断言：Finish 帧到达时每个文件都必须收齐。它看的是内存 bitmap，
                // 与「DB bitmap 只在 publish 后才完整」这条持久化纪律无关，两者不冲突。
                //
                // **位置很关键**：必须在 `try_join` 之后。收帧循环读到 `Finish` 只说明
                // 对端不再发了，队列里可能还压着未消化的块——在收帧循环里断言会误报未收齐。
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
                Ok(true)
            }
        }
    }

    /// 收帧循环：独占整条流，把数据块塞进队列，就地应答流控窗口。
    ///
    /// 它**不碰任何持久化状态**——那些全归 [`run_digest_loop`](Self::run_digest_loop)。
    /// 这条分工不是风格，是拆成两条并发路径之后唯一需要的同步纪律，用所有权钉死。
    async fn run_frame_loop(
        &self,
        stream: &mut P2pStream,
        epoch: i64,
        mut queue: mpsc::Sender<PendingBlock>,
    ) -> AppResult<FrameLoopEnd> {
        let mut probe = FrameProbe::new("recv-frame", self.session_id, FRAME_LABELS);
        loop {
            if self.cancel_token.is_cancelled() {
                return Ok(FrameLoopEnd::Cancelled);
            }

            // 空闲等待下一帧时也响应取消，避免 cancel 后干等到下一帧或超时。
            probe.mark();
            let frame = tokio::select! {
                _ = self.cancel_token.cancelled() => return Ok(FrameLoopEnd::Cancelled),
                frame = read_frame(&mut *stream) => frame?,
            };
            probe.lap(FRAME_WAIT);
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
                    // **背压就在这一个 await 上**：队列满则挂起，于是不再读流，对端的字节
                    // 堆在传输层，其流控自然回压到发送端。`send` 出错只可能是消化端已经
                    // 退出（几乎总是因为它自己报了错），真正的错误由 `try_join` 从那边带出，
                    // 这里的文案只是兜底。
                    queue
                        .send(PendingBlock { range, proof })
                        .await
                        .map_err(|_| AppError::Transfer("消化端已退出，无法继续收块".into()))?;
                    probe.lap(FRAME_ENQUEUE);
                    probe.block_done(length);
                }
                // **只是「对端不再发了」，不等于数据已消化完**——队列里可能还压着块。
                // 收齐判定与 Finish 确认都留给调用方在两条路径都收敛之后做。
                Some(TransferDataFrame::Finish {
                    session_id,
                    epoch: frame_epoch,
                }) if session_id == self.session_id && EpochGuard::matches(frame_epoch, epoch) => {
                    return Ok(FrameLoopEnd::Finished);
                }
                // 流控窗口确认。**就地回，不等消化**——这正是流水线的关键一步。
                //
                // 拆分之前这里必须等窗内每块都落盘才能回，理由是「否则在途量失控」。那条
                // 约束现在由队列容量承担：队列满 → 上面那个 `send` 挂起 → 读不到下一帧
                // `Window` → 不回确认 → 对端停在窗口边界。**闭环仍在应用层**，没有一步
                // 依赖传输层背压（那在浏览器上并不存在）。推导见 [`DIGEST_QUEUE_CHUNKS`]。
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

    /// 消化循环：从队列取块，验签 → 落盘 → checkpoint → 收齐即发布。
    ///
    /// 队列被收帧循环 drop（正常结束或取消）后 `next()` 返回 `None`，本循环随之收敛并
    /// 交还它独占的两张表。
    async fn run_digest_loop(
        &self,
        mut queue: mpsc::Receiver<PendingBlock>,
        progress: &Arc<Mutex<ProgressTracker>>,
        mut bitmaps: HashMap<u32, Vec<u8>>,
        is_resume: bool,
    ) -> AppResult<Digested> {
        let mut sinks: HashMap<u32, FileSinkId> = HashMap::new();
        let mut started_files = HashSet::new();
        let mut probe = DigestProbe::new("recv", self.session_id, DIGEST_LABELS);
        loop {
            // 取消时**丢掉队列里剩下的块**直接收敛。它们尚未落盘，checkpoint 里也就没有
            // 它们——对端按 bitmap 重发即可，与中断恢复走同一条路径。
            if self.cancel_token.is_cancelled() {
                break;
            }
            probe.mark();
            let Some(block) = queue.next().await else {
                break;
            };
            probe.lap(DIGEST_QUEUE);
            let length = block.range.length;
            self.handle_block_data(
                progress,
                &mut sinks,
                &mut started_files,
                &mut bitmaps,
                is_resume,
                block.range,
                block.proof,
                &mut probe,
            )
            .await?;
            probe.block_done(length);
        }
        Ok(Digested { sinks, bitmaps })
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
        probe: &mut DigestProbe,
    ) -> AppResult<()> {
        let (file_info, data) = self.verify_block(&range, proof)?;
        probe.lap(DIGEST_VERIFY);
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
        // 建 sink 归 `rest`，**不能让它默默滚进 `write`**：每个文件的首块要走
        // `create_sink`，在 Android 上那是一次 SAF 文档创建（慢路径）。而 `write` 那一桶的
        // 全部价值在于「只」反映闪存写入代价——掺进建 sink 的成本，几万个小文件的会话就会
        // 显示出一个根本不存在的写盘瓶颈，正好误导这个探针存在的目的。
        probe.lap(DIGEST_REST);
        let completed_bitmap = self
            .persist_chunk(&file_info, &sink_id, &range, data, bitmaps, probe)
            .await?;
        self.emit_chunk_progress(progress, &range).await;
        // `rest` 收 ensure_sink、bitmap 簿记与进度事件；发布单独成桶——它与前三者差着
        // 好几个数量级（SAF 目标是全量拷贝），混在一起真机日志就分不出是哪一段慢。
        probe.lap(DIGEST_REST);
        if let Some(bitmap) = completed_bitmap {
            self.publish_file(&file_info, sinks, bitmap).await?;
            probe.lap(DIGEST_PUBLISH);
        }
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
    ///
    /// **借 `&FileInfo` 而不是 `.cloned()`**：这里是每 256 KiB 走一次的热路径，而 `FileInfo`
    /// 带三个 `String`——7.49 GiB 的会话约 3 万块，克隆等于 9 万次无谓堆分配，而下游全部
    /// 按引用读。生命周期绑在 `&self.files` 上，与调用方后续那些 `&self` 方法是共享借用，
    /// 并存无碍。
    fn verify_block(
        &self,
        range: &FileRange,
        proof: Option<Vec<u8>>,
    ) -> AppResult<(&FileInfo, Vec<u8>)> {
        let file_info = self
            .files
            .iter()
            .find(|file| file.file_id == range.file_id)
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: {}", range.file_id)))?;
        validate_block_range(file_info, range)?;

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
                p.progress_event(false)
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
        probe: &mut DigestProbe,
    ) -> AppResult<Option<Vec<u8>>> {
        self.file_access
            .write_sink_chunk(sink_id, range.offset, data)
            .await?;
        // 写盘单列：症状 B 若真出在接收设备的闪存侧（pSLC 耗尽 / GC），
        // 增长会**只**出现在这一段。
        probe.lap(DIGEST_WRITE);

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
        probe.lap(DIGEST_CKPT);
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

        // 发布在各端代价天差地别：桌面 / iOS 是同卷重命名、Web 是 OPFS close，都 O(1)；
        // Android 的 SAF 目标是全量字节拷贝（6 GB 文件要写 12 GB）。字节已收完、进度条已满，
        // 若这段静默，用户看到的就是「满了之后凭空多等几十秒」——而用户对静止的解读是卡死。
        //
        // **发布中的字节数不从这里报**：那个循环在移动端 JS 侧的宿主适配器里，由它直接上报。
        // 为一个平台的慢路径给三端共用的 `FileAccess` 端口加回调参数是反向的。
        self.emit_publish_phase(file_info, FilePublishPhase::Started)
            .await;
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
        // `Finished` 只能发在这里：夹在 `finalize_sink` 与 `mark_file_completed` 之间就
        // 破坏了上面那条不变量（发事件是个 await 点）。
        self.emit_publish_phase(file_info, FilePublishPhase::Finished)
            .await;
        self.remove_created_sink(&sink_id).await;
        Ok(())
    }

    /// 广播一个文件级发布事件。
    ///
    /// **小文件直接跳过**（判据 [`PUBLISH_ANNOUNCE_MIN_BYTES`]）：这条事件存在的唯一目的是
    /// 解释一段**久到用户会以为卡死**的等待，而三端都只在发布持续超过 300ms 后才揭示它。
    /// 小文件的发布在任何存储上都远快于那个阈值，事件必然被前端原样丢弃——发了只是让一个
    /// 几万文件的会话白白多推几万条 IPC 消息，而 `emit_best_effort` 就 await 在收块热路径上。
    ///
    /// 零字节是这条判据的极端情形（它们由
    /// [`publish_pending_empty_files`](Self::publish_pending_empty_files) 统一补发布，
    /// 连一次写都没有）。用尺寸判而不是给 `publish_file` 加一个「要不要广播」的开关——
    /// 「这次发布有没有过程可展示」本来就是尺寸的函数。
    ///
    /// 失败路径**不发事件**：`publish_file` 的 `?` 会冒泡成可恢复的 Interrupted，前端靠
    /// 既有的会话级终态/暂停事件清掉发布态即可，不必再造一个只有一处消费的失败变体。
    async fn emit_publish_phase(&self, file_info: &FileInfo, phase: FilePublishPhase) {
        if file_info.size < PUBLISH_ANNOUNCE_MIN_BYTES {
            return;
        }
        self.emit_best_effort(
            TransferEvent::FilePublish {
                event: FilePublishEvent {
                    session_id: self.session_id,
                    file_id: file_info.file_id,
                    name: file_info.name.clone(),
                    relative_path: file_info.relative_path.clone(),
                    total_bytes: file_info.size,
                    phase,
                },
            },
            "上报文件发布阶段",
        )
        .await;
    }

    /// 累计已传输字节并发进度事件。
    ///
    /// 这一块让**整个会话**收齐时**强制发帧**：末块与节流窗口相比太快，否则最后那帧 100%
    /// 几乎必然被丢掉，UI 停在 99.x% 后直接跳完成。
    ///
    /// 按**文件**判会退化成 O(N²)（任何 ≤`CHUNK_SIZE` 的文件都只有一块 ⇒ 每个小文件都强制
    /// 一帧，而每帧都克隆整个 `files` 向量），推导见
    /// [`ProgressTracker::update_file_chunk`](crate::progress::ProgressTracker::update_file_chunk)。
    async fn emit_chunk_progress(&self, progress: &Arc<Mutex<ProgressTracker>>, range: &FileRange) {
        let progress_event = {
            let mut p = progress.lock().await;
            p.add_bytes(range.length);
            let session_completed = p.update_file_chunk(range.file_id, range.length);
            p.progress_event(session_completed)
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
