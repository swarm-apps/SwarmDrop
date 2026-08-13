//! 发送方 actor（SenderActor）
//!
//! 管理单个发送传输的生命周期：经数据面裸流推送文件块、处理 Cancel。
//! 文件读取通过 [`FileAccess`] trait 完成。wire v2 已删应用层加密——Noise/TLS 在途
//! 已加密，数据面直接传明文（见 [`wire`](crate::wire)）。
//! 使用 `Arc<std::sync::Mutex<ProgressTracker>>` 实现并发安全的进度追踪。
//!
//! # 备块 ‖ 发帧
//!
//! 推送是**两条并发路径**：[`encode_blocks`]（读源 + 建 bao proof）与
//! [`SenderActor::run_frame_loop`]（独占流 + 停等窗口 + 进度），中间一条有界队列。
//! 与接收端的收帧 ‖ 消化互为镜像，推导见 `openspec/changes/pipeline-send-path/design.md`。

use n0_future::time::Instant;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use futures::{SinkExt as _, StreamExt as _};
use swarmdrop_net::NodeId;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::coordinator::{ActorReport, CoordinatorInput, NetworkSignal, TransferCoordinator};
use crate::epoch::EpochGuard;
use crate::events::{TransferEvent, TransferEventSink};
use crate::host::FileAccess;
use crate::manager::PreparedFile;
use crate::probe::{
    SEND_ENQUEUE, SEND_FRAME_ACK, SEND_FRAME_LABELS, SEND_FRAME_QUEUE, SEND_FRAME_REST,
    SEND_FRAME_WRITE, SEND_LABELS, SEND_PROOF, SEND_READ, SendFrameProbe, SendProbe,
};
use crate::progress::{FileDesc, ProgressTracker, RuntimeTransferDirection};
use crate::protocol::{FileInfo, FileRange};
use crate::store::SessionStore;
use crate::wire::data_frame::{
    TransferDataFrame, TransferDataRole, manifest_digest, read_frame, write_frame,
};
use crate::{AppError, AppResult, WINDOW_CHUNKS, blocks_in_range};

/// 备块 → 发帧之间那条有界队列的容量。
///
/// **深度由抖动决定，不由窗口决定**——这正是它与接收端 `DIGEST_QUEUE_CHUNKS` 的全部差别。
/// 那边取满一个窗口是硬要求（队列装得下整窗，收帧循环才能立即回 `Window` 放行下一窗）；
/// 这边的窗口节奏由本端 [`WindowPacer`] 决定，**不经队列**，所以队列的唯一职责是盖住
/// 「备一块」与「发一块」之间的抖动。稳态下 1 就够（一块在写、一块在备），取 2 留一格。
///
/// 再深只有一个假想收益：窗口 RTT 期间让备块跑得更远。但 `ack` 从来不是大头（一窗 4 MiB，
/// 2 GB 只停 512 次），**没有实测支撑就不加深度**——每格是常驻 ~264 KiB，移动端要付。
const PREPARE_QUEUE_CHUNKS: usize = 2;

/// 已备好、等待写出的一块。
///
/// 只带 `proof`：wire v2 起明文就在 proof 的 bao 切片里，`BlockData.data` 恒空。
/// 与接收端的 `PendingBlock` 形状相同但**刻意不共用**——那边的 `proof` 是
/// `Option`（`None` 是待检出的协议违规），这边恒 `Some`，合并只会把这个区别抹掉。
struct PreparedBlock {
    range: FileRange,
    proof: Vec<u8>,
}

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
    ///
    /// # 泛型而不是写死 `P2pStream`
    ///
    /// 让两条窗口护栏测试能端到端驱动整条通道（含 Hello 与 Finish），而不是只驱动某个
    /// 内部方法——`WINDOW_CHUNKS` 那条断言正是整条数据面唯一跑得进 CI 的守卫。
    pub async fn run_data_channel<S>(
        &self,
        epoch: i64,
        mut channel: S,
        fetch_plan: Vec<FileRange>,
    ) -> AppResult<()>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        // 计划级校验**一次**，而不是每块撞一次——与接收端
        // `ReceiverActor::validate_fetch_plan` 同位同理由。
        self.validate_fetch_plan(&fetch_plan)?;
        let manifest = self.file_manifest();

        if let Err(error) = write_frame(
            &mut channel,
            &TransferDataFrame::Hello {
                session_id: self.session_id,
                epoch,
                role: TransferDataRole::Sender,
                manifest_digest: manifest_digest(&manifest),
                fetch_plan: fetch_plan.clone(),
            },
        )
        .await
        {
            return Err(self.prefer_remote_abort(&mut channel, epoch, error).await);
        }

        // 备块与发帧并发跑，中间隔一条有界队列（[`PREPARE_QUEUE_CHUNKS`]）。
        //
        // # 为什么要拆
        //
        // 拆之前一块的三件事严格串行：读源 → 建 proof → 写帧。串行本身两端都有，但代价
        // 只在浏览器那侧显形——Android 发送时「读 + 算」是原生文件读加 NEON blake3，相对
        // 网络几乎免费；浏览器发送时是 `File.slice().array_buffer()` 的 promise 往返加无
        // SIMD 的 wasm blake3，**这段时间完全不与网络写重叠**。2026-08-12 的真机实测正好
        // 落在这个形状上：同一条局域网、同一个传输，手机发 20 MB/s、浏览器发 9 MB/s。
        //
        // # 并发而非并行
        //
        // `join` 在**同一个任务**里驱动两条 future 交错前进，不 spawn、不要求 `Send`，
        // 也**没有 split 任何流**（`channel` 整条归发帧循环独占，备块循环碰都碰不到它）。
        // wasm 单线程一样成立：能重叠的是「已交出去、在本线程之外跑的事」——读源的 JS
        // promise 与写流的浏览器网络栈。`encode_proof` 是同步 CPU，**它是这条流水线的
        // 天花板**，谁也压不住。
        //
        // # 为什么是 `join` 而不是 `try_join`
        //
        // `try_join` 会在备块端出错时**就地 drop 正在 `write_frame` 的发帧循环**，流上
        // 留下半截帧；随后下面那条 `prefer_remote_abort` 往同一条流写读，对端读到的是
        // 「残帧 + Abort」的字节流，解码成一句与真因无关的垃圾错误——真因当场丢失。
        // 这里 `join` 是正确性要求，不是取舍。
        let (block_tx, block_rx) = mpsc::channel(PREPARE_QUEUE_CHUNKS);
        let (encode_result, frame_result) = futures::future::join(
            encode_blocks(
                self.session_id,
                &self.files,
                &*self.file_access,
                &fetch_plan,
                &self.cancel_token,
                block_tx,
            ),
            self.run_frame_loop(&mut channel, epoch, block_rx),
        )
        .await;

        // **发帧端的错误先抛**：备块端失败时发帧端只看到「队列关了」→ 正常返回 `Ok`，
        // 真因在 `encode_result`；发帧端失败时备块端只看到「发帧端已退出」这句次生文案，
        // 真因在 `frame_result`。两种情形下先抛 `frame_result` 都落在真因上。
        // （接收端「消化端先抛」是同一条规则的另一次应用：抛那条不是在观察对方退场的。）
        if let Err(error) = frame_result {
            return Err(self.prefer_remote_abort(&mut channel, epoch, error).await);
        }
        // **Finish 只能写在这之后。** 备块端中途失败会让发帧端「正常」收敛，此时若把它
        // 当成传完了，对端就会在只收到一半的情况下被告知完成。
        encode_result?;

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
    async fn prefer_remote_abort<S>(
        &self,
        channel: &mut S,
        epoch: i64,
        original: AppError,
    ) -> AppError
    where
        S: futures::io::AsyncRead + Unpin,
    {
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

    /// 计划级校验：一次计划校验一次，好过每块撞一次。
    ///
    /// 与接收端 [`ReceiverActor::validate_fetch_plan`] 同位同判据。**「非空文件的零长
    /// range 直接拒」这条是载荷性的**——[`blocks_in_range`] 把「`length == 0` ⟹ 一个空块」
    /// 当作全定义域规则，少了这道关，一个非空文件就会收到一个空块：接收端既写不进东西
    /// 也推不动 bitmap，会话卡在「对端说发完了、位图却不完整」上。
    ///
    /// [`ReceiverActor::validate_fetch_plan`]: crate::actor::receiver::ReceiverActor::validate_fetch_plan
    fn validate_fetch_plan(&self, fetch_plan: &[FileRange]) -> AppResult<()> {
        for range in fetch_plan {
            let file = self.file(range.file_id)?;
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
            if file.size > 0 && range.length == 0 {
                return Err(AppError::Transfer(format!(
                    "非空文件的 fetch range 长度为 0: file_id={}",
                    range.file_id
                )));
            }
        }
        Ok(())
    }

    fn file(&self, file_id: u32) -> AppResult<&PreparedFile> {
        self.files
            .iter()
            .find(|f| f.file_id == file_id)
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: file_id={file_id}")))
    }

    /// 发帧循环：**独占整条流**，从队列取已备好的块写出去，满窗就地停等。
    ///
    /// 它不碰源文件、不算 proof——那些全归 [`encode_blocks`]。窗口簿记（[`WindowPacer`]）
    /// 是它的局部变量，于是「在途量越限」这件事不再依赖任何跨方法的纪律：整条数据面
    /// 只有这一处写块。
    async fn run_frame_loop<S>(
        &self,
        stream: &mut S,
        epoch: i64,
        mut queue: mpsc::Receiver<PreparedBlock>,
    ) -> AppResult<()>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        let mut pacer = WindowPacer::default();
        // 逐阶段耗时探针。汇总由 Drop 打，所以下面每条 early return 都会带出数据。
        let mut probe = SendFrameProbe::new("send-frame", self.session_id, SEND_FRAME_LABELS);
        loop {
            if self.cancel_token.is_cancelled() {
                return Err(AppError::Transfer("传输已取消".into()));
            }
            probe.mark();
            // 队列关闭 = 备块端收敛（成功或失败）。**这里不写 Finish**——它由
            // `run_data_channel` 在两条 result 都 `Ok` 之后写，理由见那里。
            let Some(block) = queue.next().await else {
                return Ok(());
            };
            probe.lap(SEND_FRAME_QUEUE);
            let (file_id, length) = (block.range.file_id, block.range.length);

            write_frame(
                stream,
                &TransferDataFrame::BlockData {
                    session_id: self.session_id,
                    epoch,
                    range: block.range,
                    data: Vec::new(),
                    proof: Some(block.proof),
                },
            )
            .await?;
            // 这一段吃掉传输层的全部背压（浏览器的 `bufferedAmount` 等待、SCTP 拥塞窗口、
            // QUIC 流控）。诊断报告的预算分解指向它，但那是**推算**——这里是实测。
            probe.lap(SEND_FRAME_WRITE);

            self.record_block_sent(file_id, length).await?;
            probe.lap(SEND_FRAME_REST);

            // 停等窗口的等待计在自己的阶段里：它与 `write` 混在一起就分不出「传输层背压」
            // 和「对端消化不过来」——两者的修法完全不同。
            if pacer.count_block() {
                let synced = self.sync_window(stream, epoch).await;
                probe.lap(SEND_FRAME_ACK);
                synced?;
            }
            probe.block_done(length);
        }
    }

    /// 一块确实写出去之后的簿记：刷新空闲时钟、累计进度、按节流发事件。
    ///
    /// **只由发帧循环调用**——进度的语义是「已经上线的字节」，不是「已经备好的字节」。
    async fn record_block_sent(&self, file_id: u32, length: u64) -> AppResult<()> {
        self.last_activity_ms.store(
            self.created_at.elapsed().as_millis() as u64,
            Ordering::Relaxed,
        );

        let progress_event = {
            let mut p = self
                .progress
                .lock()
                .map_err(|_| AppError::Transfer("ProgressTracker 锁中毒".into()))?;
            p.add_bytes(length);
            // **会话**收齐那一帧强制穿过节流：末块与节流窗口相比太快，否则最后那帧 100%
            // 会被丢掉，UI 停在 99.x% 直接跳完成。按**文件**判会退化成 O(N²)，理由见
            // `ProgressTracker::update_file_chunk` 的文档。
            let session_completed = p.update_file_chunk(file_id, length);
            p.progress_event(session_completed)
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

/// 备块循环：按 fetch_plan 逐块读源、生成 bao proof，塞进队列。
///
/// # 为什么是自由函数而不是 `SenderActor` 的方法
///
/// 它要的只有源文件、宿主读口、计划、取消令牌与那条队列——**签名里没有流、没有进度、
/// 没有事件总线**（`session_id` 纯粹给探针打标）。「备块端碰不到协议与 UI 状态」这条
/// 纪律因此由类型系统保证，而不是靠约定。这是拆成两条并发路径之后唯一需要的同步纪律。
///
/// 接收端的 `run_digest_loop` 刻意**没有**这么做：它要 store / events / progress /
/// bitmaps / sinks，自由函数得吃八个参数，留在 `&self` 上是对的。**两边的不对称来自真实
/// 依赖集不同，不是风格不统一**——别去「统一」它。
async fn encode_blocks(
    session_id: Uuid,
    files: &[PreparedFile],
    file_access: &dyn FileAccess,
    fetch_plan: &[FileRange],
    cancel: &CancellationToken,
    mut queue: mpsc::Sender<PreparedBlock>,
) -> AppResult<()> {
    // 汇总由 Drop 打，所以下面每条 early return 都会带出数据。
    let mut probe = SendProbe::new("send", session_id, SEND_LABELS);
    for range in fetch_plan {
        let file = files
            .iter()
            .find(|f| f.file_id == range.file_id)
            .ok_or_else(|| AppError::Transfer(format!("文件不存在: file_id={}", range.file_id)))?;
        // 每个 range 解析一次 checksum，而不是每块一次——它是十六进制解析，与块数同阶。
        let root = crate::bao::root_from_checksum(&file.checksum)?;

        for (offset, length) in blocks_in_range(range.offset, range.length) {
            if cancel.is_cancelled() {
                return Err(AppError::Transfer("传输已取消".into()));
            }
            probe.mark();
            let plaintext = file_access
                .read_source_chunk(&file.source_id, offset, length)
                .await?;
            probe.lap(SEND_READ);
            // range 已按 file.size 校验，EOF 短读在此不可能——长度不符只能是宿主违约
            // 或文件在传输中被外部改动。静默发出缩短的块会让块游标仍按请求 len 推进，
            // 留下永不补发的 gap（接收端 checkpoint 永远收不齐），必须响错。
            if plaintext.len() != length {
                return Err(AppError::Transfer(format!(
                    "read_source_chunk 返回长度异常: 请求 {length}B@{offset}，得到 {}B (file_id={})",
                    plaintext.len(),
                    file.file_id
                )));
            }

            // 逐块验签（Approach B）：proof 携带该 range 的完整 bao 切片（含叶子），data 置空——
            // 叶子只在 proof 出现一次，无 2x 冗余。接收端 decode 必然验签，验过即写盘。
            let proof =
                crate::bao::encode_proof(&file.outboard, root, file.size, offset, &plaintext)?;
            probe.lap(SEND_PROOF);

            // **背压就在这一个 await 上**：队列满则挂起，于是不再读源，内存占用有界。
            // `send` 出错只可能是发帧端已经退出（几乎总是因为它自己报了错），真正的错误
            // 由 `run_data_channel` 从那边带出，这里的文案只是兜底。
            queue
                .send(PreparedBlock {
                    range: FileRange {
                        file_id: file.file_id,
                        offset,
                        length: length as u64,
                    },
                    proof,
                })
                .await
                .map_err(|_| AppError::Transfer("发帧端已退出，无法继续备块".into()))?;
            probe.lap(SEND_ENQUEUE);
            probe.block_done(length as u64);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::pin::Pin;
    use std::sync::atomic::AtomicUsize;
    use std::task::{Context, Poll};
    use std::time::Duration;

    use futures::io::Cursor as IoCursor;
    use swarmdrop_net::SecretKey;

    use super::*;
    use crate::CHUNK_SIZE;
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

    /// 按脚本回放读侧字节的对端替身；写入全部录下来。
    ///
    /// 脚本放完之后读侧转 `Pending` 而**不是** EOF：EOF 会被 `read_frame` 读成「通道提前
    /// 关闭」，把「发送端多读了一帧」这类真实缺陷伪装成传输层故障。
    #[derive(Clone)]
    struct ScriptedPeer {
        written: Arc<Mutex<Vec<u8>>>,
        script: Arc<Vec<u8>>,
        pos: usize,
    }

    impl ScriptedPeer {
        fn new(script: Vec<u8>) -> Self {
            Self {
                written: Arc::new(Mutex::new(Vec::new())),
                script: Arc::new(script),
                pos: 0,
            }
        }
    }

    impl futures::io::AsyncWrite for ScriptedPeer {
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

    impl futures::io::AsyncRead for ScriptedPeer {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            if this.pos >= this.script.len() {
                return Poll::Pending;
            }
            let n = buf.len().min(this.script.len() - this.pos);
            buf[..n].copy_from_slice(&this.script[this.pos..this.pos + n]);
            this.pos += n;
            Poll::Ready(Ok(n))
        }
    }

    /// 写到第 `budget` 个字节就断的对端替身；读侧立即 EOF（让 `prefer_remote_abort` 不必干等）。
    #[derive(Clone)]
    struct BrokenPeer {
        written: Arc<Mutex<Vec<u8>>>,
        budget: usize,
    }

    impl futures::io::AsyncWrite for BrokenPeer {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let mut written = self.written.lock().unwrap();
            if written.len() >= self.budget {
                return Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "对端已断开")));
            }
            let n = buf.len().min(self.budget - written.len());
            written.extend_from_slice(&buf[..n]);
            Poll::Ready(Ok(n))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl futures::io::AsyncRead for BrokenPeer {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(0))
        }
    }

    /// 读取次数计数器 + 一个可被唤醒的挂起点。
    ///
    /// 存在的唯一目的是让 [`pipelines_reading_ahead_of_writing`] 能表达「写还没完成时，
    /// 下一块的读必须已经发生」——那是这次流水线化的**中心不变量**，而它在任何只看
    /// 输出字节的断言下都不可见。
    #[derive(Default)]
    struct Gate {
        reads: AtomicUsize,
        waker: std::sync::Mutex<Option<std::task::Waker>>,
    }

    impl Gate {
        fn record_read(&self) -> usize {
            let seq = self.reads.fetch_add(1, Ordering::SeqCst);
            if let Some(waker) = self.waker.lock().unwrap().take() {
                waker.wake();
            }
            seq
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }

        fn park(&self, cx: &Context<'_>) {
            *self.waker.lock().unwrap() = Some(cx.waker().clone());
        }
    }

    /// 把「写」按住不放，直到源文件被读过 `open_at` 次的对端替身。
    ///
    /// **串行实现在这里必然死锁**：它写完当前块才会去读下一块，而写正被按住。
    /// 流水线实现则会在写挂起期间继续备下一块，把门推开。脚本部分与
    /// [`ScriptedPeer`] 同义（放完转 `Pending`）。
    #[derive(Clone)]
    struct GatedPeer {
        written: Arc<Mutex<Vec<u8>>>,
        script: Arc<Vec<u8>>,
        pos: usize,
        gate: Arc<Gate>,
        open_at: usize,
    }

    impl futures::io::AsyncWrite for GatedPeer {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            // 一次读都还没发生 = 还在写 Hello，放行；否则等门开。
            let reads = self.gate.reads();
            if reads > 0 && reads < self.open_at {
                self.gate.park(cx);
                return Poll::Pending;
            }
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

    impl futures::io::AsyncRead for GatedPeer {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            if this.pos >= this.script.len() {
                return Poll::Pending;
            }
            let n = buf.len().min(this.script.len() - this.pos);
            buf[..n].copy_from_slice(&this.script[this.pos..this.pos + n]);
            this.pos += n;
            Poll::Ready(Ok(n))
        }
    }

    /// 按偏移切一份内存里的源数据；事件全丢弃。
    ///
    /// `fail_at_block` 给了就在第 N 次读取（0 起）上注入失败——用来验「备块端失败时
    /// 绝不写 Finish」。
    struct MemorySource {
        data: Vec<u8>,
        fail_at_block: Option<usize>,
        gate: Arc<Gate>,
    }

    impl MemorySource {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                fail_at_block: None,
                gate: Arc::new(Gate::default()),
            }
        }

        fn failing_at(data: Vec<u8>, block: usize) -> Self {
            Self {
                data,
                fail_at_block: Some(block),
                gate: Arc::new(Gate::default()),
            }
        }
    }

    #[async_trait::async_trait]
    impl FileAccess for MemorySource {
        async fn read_source_chunk(
            &self,
            _source: &FileSourceId,
            offset: u64,
            length: usize,
        ) -> AppResult<Vec<u8>> {
            let seq = self.gate.record_read();
            if self.fail_at_block == Some(seq) {
                return Err(AppError::Transfer("注入的读失败".into()));
            }
            let start = offset as usize;
            Ok(self.data[start..start + length].to_vec())
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

    fn prepared(data: &[u8]) -> PreparedFile {
        let (root, outboard) = crate::bao::build_outboard(data);
        PreparedFile {
            file_id: 0,
            name: "big.bin".into(),
            relative_path: "big.bin".into(),
            source_id: FileSourceId("mem".into()),
            size: data.len() as u64,
            checksum: root.to_hex().to_string(),
            outboard,
        }
    }

    fn sender(session_id: Uuid, data: &[u8], host: Arc<MemorySource>) -> SenderActor {
        SenderActor::new(
            session_id,
            SecretKey::generate().node_id(),
            vec![prepared(data)],
            host.clone(),
            host,
        )
    }

    fn full_plan(data: &[u8]) -> Vec<FileRange> {
        vec![FileRange {
            file_id: 0,
            offset: 0,
            length: data.len() as u64,
        }]
    }

    async fn frames_of(bytes: Vec<u8>) -> Vec<TransferDataFrame> {
        let mut cursor = IoCursor::new(bytes);
        let mut frames = Vec::new();
        while let Some(frame) = read_frame(&mut cursor).await.unwrap() {
            frames.push(frame);
        }
        frames
    }

    fn count(frames: &[TransferDataFrame], f: impl Fn(&TransferDataFrame) -> bool) -> usize {
        frames.iter().filter(|frame| f(frame)).count()
    }

    fn blocks(frames: &[TransferDataFrame]) -> usize {
        count(frames, |f| matches!(f, TransferDataFrame::BlockData { .. }))
    }

    fn windows(frames: &[TransferDataFrame]) -> usize {
        count(frames, |f| matches!(f, TransferDataFrame::Window { .. }))
    }

    fn finishes(frames: &[TransferDataFrame]) -> usize {
        count(frames, |f| matches!(f, TransferDataFrame::Finish { .. }))
    }

    /// 数据面流控的护栏：**满一窗必须停下等确认**。
    ///
    /// 去掉窗口后传输在 native↔native 之间照样跑得通（那边 yamux/QUIC 顶着），只有
    /// 浏览器接收端会在几 MiB 处被撑爆——那是跑不进 CI 的失效模式，所以这条断言是它
    /// 唯一的机器守卫。改 `WINDOW_CHUNKS` 不会让它红（它按常量算期望值），删掉窗口会。
    ///
    /// 流水线化之后备块端会比发帧端跑前 [`PREPARE_QUEUE_CHUNKS`] 块，**但写出去的帧数
    /// 不受队列影响**——这条断言看的是流上的字节，正是它该看的东西。
    #[tokio::test]
    async fn sender_stops_after_one_window_until_peer_acks() {
        let total_chunks = WINDOW_CHUNKS + 1;
        let data = vec![7u8; total_chunks as usize * CHUNK_SIZE];
        let host = Arc::new(MemorySource::new(data.clone()));
        let actor = sender(Uuid::new_v4(), &data, host);

        let peer = StalledPeer::default();
        let outcome = n0_future::time::timeout(
            Duration::from_millis(500),
            actor.run_data_channel(1, peer.clone(), full_plan(&data)),
        )
        .await;
        assert!(
            outcome.is_err(),
            "对端不回确认时，发送端必须停在窗口边界，而不是把整个文件推出去"
        );

        let written = peer.0.lock().unwrap().clone();
        let frames = frames_of(written).await;
        assert!(
            matches!(frames.first(), Some(TransferDataFrame::Hello { .. })),
            "第一帧必须是 Hello"
        );
        assert_eq!(
            blocks(&frames),
            WINDOW_CHUNKS as usize,
            "停下前恰好推满一窗"
        );
        assert_eq!(windows(&frames), 1, "并且已经发出窗口同步帧");
        assert_eq!(finishes(&frames), 0, "还没传完，不该出现 Finish");
    }

    /// 数据面流控的另一半：**收到确认后必须继续推完**，并以 Finish 往返收尾。
    ///
    /// 与 [`sender_stops_after_one_window_until_peer_acks`] 配对——只有那一条时，把
    /// `sync_window` 写成「永远返回 Pending」也能让它通过，而那会把每一次传输都卡死在
    /// 第一窗。
    #[tokio::test]
    async fn sender_resumes_after_each_window_ack() {
        let session_id = Uuid::new_v4();
        let epoch = 3;
        // 三窗零头：确保确认路径被走满三次，而不是只在收尾时碰巧过一次。
        let total_chunks = WINDOW_CHUNKS * 3 + 1;
        let data = vec![9u8; total_chunks as usize * CHUNK_SIZE];

        let mut script = Vec::new();
        for _ in 0..3 {
            write_frame(
                &mut script,
                &TransferDataFrame::Window { session_id, epoch },
            )
            .await
            .unwrap();
        }
        write_frame(
            &mut script,
            &TransferDataFrame::Finish { session_id, epoch },
        )
        .await
        .unwrap();

        let host = Arc::new(MemorySource::new(data.clone()));
        let actor = sender(session_id, &data, host);
        let peer = ScriptedPeer::new(script);

        n0_future::time::timeout(
            Duration::from_secs(20),
            actor.run_data_channel(epoch, peer.clone(), full_plan(&data)),
        )
        .await
        .expect("对端持续确认时不该卡住")
        .expect("推送应当成功");

        let written = peer.written.lock().unwrap().clone();
        let frames = frames_of(written).await;
        assert_eq!(blocks(&frames), total_chunks as usize, "所有块都应推出去");
        assert_eq!(windows(&frames), 3, "满窗三次，确认三次");
        assert_eq!(finishes(&frames), 1, "收尾恰好一帧 Finish");
    }

    /// **备块端失败时绝不写 Finish。**
    ///
    /// 备块端出错会让发帧端「正常」收敛（队列关了，`next()` 返回 `None`）。若把那当成
    /// 传完了，对端就会在只收到一半的情况下被告知完成——它的 `ensure_files_complete`
    /// 会报一句与真因无关的错，而真因（源文件读不了）再也传不出去。
    #[tokio::test]
    async fn encode_failure_never_writes_finish() {
        // 三块，第二块读失败；远小于一窗，所以整条路径不涉及窗口确认。
        let data = vec![3u8; CHUNK_SIZE * 3];
        let host = Arc::new(MemorySource::failing_at(data.clone(), 1));
        let actor = sender(Uuid::new_v4(), &data, host);

        let peer = StalledPeer::default();
        let error = n0_future::time::timeout(
            Duration::from_secs(10),
            actor.run_data_channel(1, peer.clone(), full_plan(&data)),
        )
        .await
        .expect("备块端失败后两条路径都该收敛，不该挂住")
        .expect_err("备块端失败必须冒泡");
        assert!(
            error.to_string().contains("注入的读失败"),
            "上抛的必须是备块端的真因，实际: {error}"
        );

        let written = peer.0.lock().unwrap().clone();
        let frames = frames_of(written).await;
        assert_eq!(finishes(&frames), 0, "备块端失败时流上绝不能出现 Finish");
    }

    /// 两条路径都出错时，**上抛发帧端的那一个**。
    ///
    /// 备块端此时只会报「发帧端已退出」——那是次生文案，把它抛出去等于告诉用户
    /// 「内部队列断了」，而真相是对端断了连接。
    #[tokio::test]
    async fn frame_error_wins_over_encode_error() {
        let data = vec![5u8; CHUNK_SIZE * 8];
        let host = Arc::new(MemorySource::new(data.clone()));
        let actor = sender(Uuid::new_v4(), &data, host);

        // 预算够写 Hello、不够写完第一块 BlockData。
        let peer = BrokenPeer {
            written: Arc::new(Mutex::new(Vec::new())),
            budget: 1024,
        };
        let error = n0_future::time::timeout(
            Duration::from_secs(10),
            actor.run_data_channel(1, peer.clone(), full_plan(&data)),
        )
        .await
        .expect("对端断流后两条路径都该收敛")
        .expect_err("写失败必须冒泡");
        assert!(
            error.to_string().contains("transfer-data IO 错误"),
            "上抛的必须是发帧端的 IO 错误，实际: {error}"
        );
        assert!(
            !error.to_string().contains("发帧端已退出"),
            "备块端的次生文案不该盖住真因，实际: {error}"
        );
    }

    /// **这次改动的中心不变量：读跑在写前面。**
    ///
    /// 对端把第一块的写按住不放，直到源文件被读过第二次。串行实现（读 → 算 → 写 →
    /// 读下一块）在这里**必然死锁**：它要等写完成才会去读第二块，而写正在等第二次读。
    /// 流水线实现会在写挂起期间继续备下一块，把门推开。
    ///
    /// 没有这条断言，把两条路径改回一条串行链能通过其余全部测试——输出的字节一模一样，
    /// 只是慢了一倍。而「慢了一倍」在 CI 里没有任何形状。
    #[tokio::test]
    async fn pipelines_reading_ahead_of_writing() {
        let session_id = Uuid::new_v4();
        let epoch = 11;
        // 四块，远小于一窗，整条路径不涉及窗口确认。
        let data = vec![4u8; CHUNK_SIZE * 4];
        let mut script = Vec::new();
        write_frame(
            &mut script,
            &TransferDataFrame::Finish { session_id, epoch },
        )
        .await
        .unwrap();

        let host = Arc::new(MemorySource::new(data.clone()));
        let gate = host.gate.clone();
        let actor = sender(session_id, &data, host);
        let peer = GatedPeer {
            written: Arc::new(Mutex::new(Vec::new())),
            script: Arc::new(script),
            pos: 0,
            gate,
            open_at: 2,
        };

        n0_future::time::timeout(
            Duration::from_secs(5),
            actor.run_data_channel(epoch, peer.clone(), full_plan(&data)),
        )
        .await
        .expect("发送端必须在写挂起期间继续备下一块——串行实现会在这里死锁")
        .expect("门开之后推送应当成功");

        let written = peer.written.lock().unwrap().clone();
        let frames = frames_of(written).await;
        assert_eq!(blocks(&frames), 4);
        assert_eq!(finishes(&frames), 1);
    }

    /// 计划级校验：坏计划在开流之前就拒，而不是每块撞一次。
    #[test]
    fn validate_fetch_plan_rejects_bad_ranges() {
        let data = vec![1u8; CHUNK_SIZE * 2];
        let host = Arc::new(MemorySource::new(data.clone()));
        let actor = sender(Uuid::new_v4(), &data, host);

        let range = |file_id, offset, length| {
            vec![FileRange {
                file_id,
                offset,
                length,
            }]
        };
        assert!(
            actor.validate_fetch_plan(&range(9, 0, 1)).is_err(),
            "未知 file_id 必须拒"
        );
        assert!(
            actor.validate_fetch_plan(&range(0, u64::MAX, 2)).is_err(),
            "offset + length 溢出必须拒"
        );
        assert!(
            actor
                .validate_fetch_plan(&range(0, 0, data.len() as u64 + 1))
                .is_err(),
            "越过文件末尾必须拒"
        );
        // 非空文件的零长 range：放行会让 `blocks_in_range` 产出一个空块，接收端既写不进
        // 东西也推不动 bitmap，会话卡在「对端说发完了、位图却不完整」上。
        assert!(
            actor.validate_fetch_plan(&range(0, 0, 0)).is_err(),
            "非空文件的零长 range 必须拒"
        );
        assert!(
            actor
                .validate_fetch_plan(&range(0, 0, data.len() as u64))
                .is_ok(),
            "整文件计划应当放行"
        );
    }

    /// 零字节文件的零长 range 是**唯一**合法的空块来源，必须放行。
    #[test]
    fn validate_fetch_plan_accepts_the_empty_file_range() {
        let host = Arc::new(MemorySource::new(Vec::new()));
        let actor = sender(Uuid::new_v4(), &[], host);
        assert!(
            actor
                .validate_fetch_plan(&[FileRange {
                    file_id: 0,
                    offset: 0,
                    length: 0,
                }])
                .is_ok()
        );
    }

    /// 零字节文件必须真的发出那一个空块（否则接收端不会为它建 sink、也不会发布）。
    #[tokio::test]
    async fn empty_file_still_sends_one_block() {
        let session_id = Uuid::new_v4();
        let epoch = 7;
        let mut script = Vec::new();
        write_frame(
            &mut script,
            &TransferDataFrame::Finish { session_id, epoch },
        )
        .await
        .unwrap();

        let host = Arc::new(MemorySource::new(Vec::new()));
        let actor = sender(session_id, &[], host);
        let peer = ScriptedPeer::new(script);

        n0_future::time::timeout(
            Duration::from_secs(10),
            actor.run_data_channel(
                epoch,
                peer.clone(),
                vec![FileRange {
                    file_id: 0,
                    offset: 0,
                    length: 0,
                }],
            ),
        )
        .await
        .expect("空文件不该卡住")
        .expect("空文件推送应当成功");

        let written = peer.written.lock().unwrap().clone();
        let frames = frames_of(written).await;
        assert_eq!(blocks(&frames), 1, "零字节文件恰好一个空块");
        assert_eq!(finishes(&frames), 1);
    }
}
