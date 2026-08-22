//! 文件传输进度模型和计算器。

use std::collections::VecDeque;
use std::time::Duration;
// wasm 上 std Instant panic（time not implemented），统一走 n0-future（native=tokio，wasm=web_time）
use n0_future::time::Instant;

use crate::host::CoreSaveLocation;
use serde::Serialize;
use uuid::Uuid;

use super::calc_total_chunks;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum RuntimeTransferDirection {
    Send,
    Receive,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum FileTransferStatus {
    Pending,
    Transferring,
    Completed,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileProgressInfo {
    pub file_id: u32,
    pub name: String,
    pub size: u64,
    pub transferred: u64,
    pub status: FileTransferStatus,
    #[serde(skip)]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub chunks_done: u32,
    #[serde(skip)]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub total_chunks: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferProgressEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub total_files: usize,
    pub completed_files: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub speed: f64,
    pub eta: Option<f64>,
    pub files: Vec<FileProgressInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferCompleteEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub total_bytes: u64,
    pub elapsed_ms: u64,
    pub save_location: Option<CoreSaveLocation>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferFailedEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferPausedEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferResumedEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<TransferResumedFileInfo>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferResumedFileInfo {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferDbErrorEvent {
    pub session_id: Uuid,
    pub message: String,
}

/// 对方接受 Offer 的事件 payload
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferAcceptedEvent {
    pub session_id: Uuid,
}

/// 对方拒绝 Offer 的事件 payload
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferRejectedEvent {
    pub session_id: Uuid,
    pub reason: Option<crate::protocol::OfferRejectReason>,
}

/// 单个文件的发布阶段（暂存 → 用户可见位置）。
///
/// 只有两档且都是 unit 变体：三端 codegen 都稳（specta → 字符串联合、uniffi → 无字段
/// enum），各端用查表渲染，将来加档会在查表处编译期报缺项。
///
/// **拷贝中的字节进度不在这里**：唯一会慢的发布路径是 Android 的 SAF 全量拷贝，那个循环
/// 住在移动端 JS 侧的宿主适配器里，由它直接上报自己的进度。为它给三端共用的
/// [`FileAccess`](crate::host::FileAccess) 端口加回调参数是反向的——其余平台的发布是
/// 常数时间的重命名，根本没有循环可上报。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum FilePublishPhase {
    Started,
    Finished,
}

/// 单个文件的发布事件。
///
/// **是文件级而非会话级**：发布是「收齐即发布」，一个会话里会发生多次、散布在整条传输
/// 过程中，不是末尾一次。挂到会话级的进度事件上就表达不了「现在正在保存哪一个」。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FilePublishEvent {
    pub session_id: Uuid,
    pub file_id: u32,
    /// 文件名（展示用）。
    pub name: String,
    /// 相对路径。移动端 JS 侧靠它把自己的拷贝字节数认领到正确的条目上——
    /// 它拿到的元数据里没有 session_id / file_id。
    pub relative_path: String,
    pub total_bytes: u64,
    pub phase: FilePublishPhase,
}

/// `prepare_send` 的 hash 进度事件
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PrepareProgressEvent {
    /// 用于区分并发 prepare（多用户在不同会话同时调用）
    pub prepared_id: Uuid,
    /// 当前正在 hash 的文件名
    pub current_file: String,
    /// 已完成 hash 的文件数
    pub completed_files: u32,
    /// 总文件数
    pub total_files: u32,
    /// 累积已 hash 的字节数（所有文件）
    pub bytes_hashed: u64,
    /// 总字节数（所有文件）
    pub total_bytes: u64,
}

pub struct FileDesc {
    pub file_id: u32,
    pub name: String,
    pub size: u64,
}

pub struct ProgressTracker {
    session_id: Uuid,
    direction: RuntimeTransferDirection,
    total_bytes: u64,
    transferred_bytes: u64,
    total_files: usize,
    completed_files: usize,
    files: Vec<FileProgressInfo>,
    started_at: Instant,
    samples: VecDeque<(Instant, u64)>,
    last_emit: Option<Instant>,
}

/// 低于这个速率（B/s）就不给 ETA，也不该报速率。
///
/// **导出是为了让渲染层引它而不是抄它。** 命令行那边要用同一条线决定「速率说不说得
/// 出口」——两处各写一个 `1.0` 的话，这里改成 0.5 的那天，用户会看到「`—` 剩余 30s」：
/// 速率说不出话，剩余时间却给得出，正是这条判据要避免的自相矛盾。
pub const MIN_SPEED_FOR_ETA: f64 = 1.0;

const THROTTLE_INTERVAL: Duration = Duration::from_millis(200);
const SPEED_WINDOW: Duration = Duration::from_secs(3);

impl ProgressTracker {
    pub fn new(
        session_id: Uuid,
        direction: RuntimeTransferDirection,
        total_bytes: u64,
        total_files: usize,
    ) -> Self {
        Self {
            session_id,
            direction,
            total_bytes,
            transferred_bytes: 0,
            total_files,
            completed_files: 0,
            files: Vec::new(),
            started_at: Instant::now(),
            samples: VecDeque::new(),
            last_emit: None,
        }
    }

    pub fn init_files_with_resume(
        &mut self,
        file_descs: &[FileDesc],
        resume_state: &std::collections::HashMap<u32, (u32, u64)>,
    ) {
        self.files = file_descs
            .iter()
            .map(|f| {
                let total_chunks = calc_total_chunks(f.size);
                let (chunks_done, transferred) =
                    resume_state.get(&f.file_id).copied().unwrap_or((0, 0));
                // **空文件一开始就算完成。** 它没有字节要传，进度意义上确实已经完成；
                // 落地由 `publish_pending_empty_files` 在 Finish 时补，与进度展示无关。
                //
                // 不这么算会让「会话收齐」这条判据**永远不成立**：`calc_total_chunks(0) == 1`，
                // 而续传时 fetch plan 不会为空文件产出 range ⇒ 它永远等不到自己那一「块」⇒
                // `completed_files` 差一 ⇒ `update_file_chunk` 一次都不返回 true ⇒ 末帧被
                // 200ms 节流吃掉，UI 停在 99.x% 直接跳完成。而含 `.gitkeep` / `__init__.py`
                // 的目录 + 中断续传，两者都很常见。
                let status = if f.size == 0 || chunks_done >= total_chunks {
                    FileTransferStatus::Completed
                } else if chunks_done > 0 {
                    FileTransferStatus::Transferring
                } else {
                    FileTransferStatus::Pending
                };
                FileProgressInfo {
                    file_id: f.file_id,
                    name: f.name.clone(),
                    size: f.size,
                    transferred,
                    status,
                    chunks_done,
                    total_chunks,
                }
            })
            .collect();

        self.completed_files = self
            .files
            .iter()
            .filter(|f| f.status == FileTransferStatus::Completed)
            .count();
        self.transferred_bytes = self.files.iter().map(|f| f.transferred).sum();
    }

    /// 记一块的进度。**返回「这一块让整个会话收齐了」**。
    ///
    /// 返回值是调用点用来强制发一帧进度的判据（见 [`progress_event`](Self::progress_event)）：
    /// 末块与节流窗口相比太快，不强制的话最后那帧「100%」几乎必然被丢掉，UI 停在 99.x%
    /// 后直接跳完成。判据取自这里而不是调用点各自的 bitmap 计数，是为了让「强制发帧」与
    /// 「帧里的数字」同源。
    ///
    /// ## 为什么是**会话**收齐而不是**文件**收齐
    ///
    /// 按文件判会退化成 O(N²)：`CHUNK_SIZE` 是 256 KiB，所以**任何 ≤256 KiB 的文件都只有
    /// 一块**，那一块必然「让该文件收齐」⇒ 每个小文件都强制一帧，而每帧
    /// [`progress_event`] 都克隆整个 `files` 向量并跨 IPC 序列化一遍。收一个几万文件的
    /// 目录时，光是自家事件流就能把接收吞吐吃光（`emit_best_effort` 就 await 在收块热路径上）。
    ///
    /// 而中间那些文件根本不需要强制：会话里还有别的文件在传，帧仍以 200ms 的常规节奏发着，
    /// 逐文件行至多晚一帧被修正。真正会「停在 99.x% 再跳完成」的只有**最后一个**文件——
    /// 它之后没有下一帧了。发布提示也不依赖这个：`FilePublish` 是独立事件。
    pub fn update_file_chunk(&mut self, file_id: u32, chunk_bytes: u64) -> bool {
        let Some(f) = self.files.iter_mut().find(|f| f.file_id == file_id) else {
            return false;
        };
        if f.status == FileTransferStatus::Completed {
            return false;
        }
        if f.status == FileTransferStatus::Pending {
            f.status = FileTransferStatus::Transferring;
        }
        f.transferred += chunk_bytes;
        f.chunks_done += 1;
        if f.chunks_done < f.total_chunks {
            return false;
        }
        f.status = FileTransferStatus::Completed;
        f.transferred = f.size;
        self.completed_files += 1;
        self.completed_files >= self.total_files
    }

    pub fn set_file_transferring(&mut self, file_id: u32) {
        if let Some(f) = self.files.iter_mut().find(|f| f.file_id == file_id)
            && f.status == FileTransferStatus::Pending
        {
            f.status = FileTransferStatus::Transferring;
        }
    }

    pub fn transferred_bytes(&self) -> u64 {
        self.transferred_bytes
    }

    pub fn get_file_progress(&self) -> Vec<(u32, u32, u64)> {
        self.files
            .iter()
            .map(|f| (f.file_id, f.chunks_done, f.transferred))
            .collect()
    }

    pub fn add_bytes(&mut self, bytes: u64) {
        self.transferred_bytes += bytes;
        let now = Instant::now();
        self.samples.push_back((now, self.transferred_bytes));

        // checked_sub：wasm 的 Instant 原点是页面加载时刻（performance.now()），页面开
        // 不足 SPEED_WINDOW 就传输时 `now - SPEED_WINDOW` 会下溢 panic（native 原点是
        // 系统启动，减不穿，掩盖了这个坑）。None = 全部样本都在窗口内，无需修剪。
        if let Some(cutoff) = now.checked_sub(SPEED_WINDOW) {
            while self.samples.front().is_some_and(|(t, _)| *t < cutoff) {
                self.samples.pop_front();
            }
        }
    }

    /// 滑窗内的平均速率（B/s）。**停滞超过一个窗口即归零。**
    ///
    /// `samples` 只在 [`add_bytes`](Self::add_bytes) 里修剪，所以字节一停，首尾两个样本
    /// 会原地不动地留在队列里——不看它们有多旧的话，这个函数会一直返回一个早已不成立的
    /// 速率，[`eta`](Self::eta) 跟着一起撒谎。而传输停滞恰恰是最需要如实相告的时刻：
    /// 接收方正在把收齐的文件发布到用户目标位置、对端卡住、本地磁盘 stall。
    ///
    /// 归零而不是修剪 `samples`，是因为本函数是 `&self` 且被 `eta()` 复用；修剪要 `&mut`，
    /// 会把「读一个派生值」变成「读的时候顺手改状态」。
    pub fn speed(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let (t_first, b_first) = self.samples.front().unwrap();
        let (t_last, b_last) = self.samples.back().unwrap();
        if Instant::now().duration_since(*t_last) >= SPEED_WINDOW {
            return 0.0;
        }
        let elapsed = t_last.duration_since(*t_first).as_secs_f64();
        if elapsed < 0.001 {
            return 0.0;
        }
        (b_last - b_first) as f64 / elapsed
    }

    pub fn eta(&self) -> Option<f64> {
        self.eta_at(self.speed())
    }

    /// 由一个**已经算好的**速率推 ETA。
    ///
    /// [`progress_event`](Self::progress_event) 每帧都要同时给出 speed 与 eta，走公开的
    /// `eta()` 会把滑窗连同 `Instant::now()` 再算一遍——wasm 上后者落到
    /// `performance.now()`，是一次跨 wasm↔JS 边界调用。
    fn eta_at(&self, speed: f64) -> Option<f64> {
        if speed < MIN_SPEED_FOR_ETA {
            return None;
        }
        let remaining = self.total_bytes.saturating_sub(self.transferred_bytes);
        Some(remaining as f64 / speed)
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    /// 取一帧进度事件；被节流挡下时返回 `None`。
    ///
    /// `force` 是节流的逃逸口，只给**文件边界**用：块间隔远小于
    /// [`THROTTLE_INTERVAL`]，所以「某文件刚好收齐」那一帧几乎必然被丢掉，UI 会停在 99.x%
    /// 直接跳完成。接收侧还要在这一帧之后紧接着进入发布阶段，不强制的话用户看到的是
    /// 「99.7% → 正在保存」，比不提示更费解。
    ///
    /// 强制发出的帧同样刷新 `last_emit`——它是真发出去了，后面那 200ms 就该照常静默。
    pub fn progress_event(&mut self, force: bool) -> Option<TransferProgressEvent> {
        let now = Instant::now();
        if !force
            && self
                .last_emit
                .is_some_and(|last| now.duration_since(last) < THROTTLE_INTERVAL)
        {
            return None;
        }
        self.last_emit = Some(now);

        // **一帧里的 speed 与 eta 必须同源。** 各自调一次 `speed()` 会取两次
        // `Instant::now()`，最新样本的年龄正好跨过 `SPEED_WINDOW` 边界时，两次的答案不同 ⇒
        // 发出 `{ speed: 12.4 MB/s, eta: null }` 这种自相矛盾的帧，三端渲染成
        // 「12.4 MB/s · 计算中」——正是本次改动花力气消灭的「一半诚实一半撒谎」。
        let speed = self.speed();
        Some(TransferProgressEvent {
            session_id: self.session_id,
            direction: self.direction,
            total_files: self.total_files,
            completed_files: self.completed_files,
            total_bytes: self.total_bytes,
            transferred_bytes: self.transferred_bytes,
            speed,
            eta: self.eta_at(speed),
            files: self.files.clone(),
        })
    }

    pub fn complete_event(&self, save_location: Option<CoreSaveLocation>) -> TransferCompleteEvent {
        TransferCompleteEvent {
            session_id: self.session_id,
            direction: self.direction,
            total_bytes: self.transferred_bytes,
            elapsed_ms: self.elapsed_ms(),
            save_location,
        }
    }

    pub fn failed_event(&self, error: String) -> TransferFailedEvent {
        TransferFailedEvent {
            session_id: self.session_id,
            direction: self.direction,
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::CHUNK_SIZE;

    #[test]
    fn progress_event_should_include_file_and_byte_progress() {
        let session_id = Uuid::new_v4();
        let mut tracker = ProgressTracker::new(session_id, RuntimeTransferDirection::Send, 10, 1);
        tracker.init_files_with_resume(
            &[FileDesc {
                file_id: 1,
                name: "a.txt".to_string(),
                size: 10,
            }],
            &HashMap::new(),
        );

        tracker.add_bytes(10);
        tracker.update_file_chunk(1, 10);

        let event = tracker
            .progress_event(false)
            .expect("first event is emitted");
        assert_eq!(event.session_id, session_id);
        assert_eq!(event.transferred_bytes, 10);
        assert_eq!(event.completed_files, 1);
        assert_eq!(event.files[0].status, FileTransferStatus::Completed);
    }

    /// 停滞期间速度与 ETA 必须归零，而不是把停滞前那个数字冻在界面上。
    ///
    /// 直接构造陈旧样本而不是等 3 秒：`add_bytes` 只会盖 `Instant::now()`，公开 API 里
    /// 没有别的入口能造出「旧样本」这个状态。
    #[test]
    fn stalled_transfer_should_report_zero_speed_and_no_eta() {
        let mut tracker =
            ProgressTracker::new(Uuid::new_v4(), RuntimeTransferDirection::Receive, 1_000, 1);

        // 新鲜样本：速度成立，ETA 也算得出来
        tracker.add_bytes(100);
        tracker.samples.clear();
        let now = Instant::now();
        tracker.samples.push_back((now - Duration::from_secs(1), 0));
        tracker.samples.push_back((now, 100));
        assert!(tracker.speed() > 0.0);
        assert!(tracker.eta().is_some());

        // 同样的两个样本，只是都老于一个滑窗
        let stale = now
            .checked_sub(SPEED_WINDOW + Duration::from_millis(100))
            .expect("测试机的单调时钟原点足够早");
        tracker.samples.clear();
        tracker
            .samples
            .push_back((stale - Duration::from_secs(1), 0));
        tracker.samples.push_back((stale, 100));
        assert_eq!(tracker.speed(), 0.0);
        assert!(tracker.eta().is_none());
    }

    /// 会话收齐那一帧必须穿过节流——否则 UI 停在 99.x% 后直接跳完成。
    #[test]
    fn session_completion_should_force_a_frame_through_the_throttle() {
        let size = (CHUNK_SIZE * 2) as u64;
        let mut tracker =
            ProgressTracker::new(Uuid::new_v4(), RuntimeTransferDirection::Send, size, 1);
        tracker.init_files_with_resume(
            &[FileDesc {
                file_id: 1,
                name: "big.bin".to_string(),
                size,
            }],
            &HashMap::new(),
        );

        // 第一块：没收齐，也不必强制
        assert!(!tracker.update_file_chunk(1, CHUNK_SIZE as u64));
        assert!(tracker.progress_event(false).is_some());
        // 紧接着的一帧被节流挡下
        assert!(tracker.progress_event(false).is_none());

        // 第二块收齐整个会话 → 调用点据此强制发帧
        assert!(tracker.update_file_chunk(1, CHUNK_SIZE as u64));
        let forced = tracker
            .progress_event(true)
            .expect("会话收齐那一帧必须穿过节流");
        assert_eq!(forced.completed_files, 1);
        assert_eq!(forced.files[0].status, FileTransferStatus::Completed);

        // 强制帧同样刷新了节流基准：紧随其后的普通帧照常静默
        assert!(tracker.progress_event(false).is_none());
        // 已完成的文件再来一块不算「刚刚收齐」
        assert!(!tracker.update_file_chunk(1, CHUNK_SIZE as u64));
    }

    /// **中间文件收齐不强制发帧。**
    ///
    /// 按文件判会退化成 O(N²)：`CHUNK_SIZE` 是 256 KiB，所以任何小文件都只有一块，那一块
    /// 必然「让该文件收齐」⇒ 每个小文件都强制一帧，而每帧都克隆整个 `files` 向量并跨 IPC
    /// 序列化。收一个几万文件的目录时，光是自家事件流就能把接收吞吐吃光。
    #[test]
    fn mid_session_file_completion_should_not_force_a_frame() {
        let size = CHUNK_SIZE as u64;
        let mut tracker = ProgressTracker::new(
            Uuid::new_v4(),
            RuntimeTransferDirection::Receive,
            size * 2,
            2,
        );
        tracker.init_files_with_resume(
            &[
                FileDesc {
                    file_id: 1,
                    name: "small-a.bin".to_string(),
                    size,
                },
                FileDesc {
                    file_id: 2,
                    name: "small-b.bin".to_string(),
                    size,
                },
            ],
            &HashMap::new(),
        );

        // 单块文件：这一块既让它开始也让它收齐，但会话还没完 ⇒ 不强制
        assert!(!tracker.update_file_chunk(1, size));
        // 最后一个文件收齐 ⇒ 会话收齐 ⇒ 强制
        assert!(tracker.update_file_chunk(2, size));
    }

    /// 空文件不得挡住「会话收齐」那一帧。
    ///
    /// `calc_total_chunks(0) == 1`，而续传时 fetch plan 不会为空文件产出 range ⇒ 它永远等不到
    /// 自己那一「块」。若不在初始化时就把它算作完成，`completed_files` 会永远差一，末帧被节流
    /// 吃掉，UI 停在 99.x% 直接跳完成——含 `.gitkeep` 的目录 + 中断续传即可复现。
    #[test]
    fn empty_files_should_not_block_session_completion() {
        let size = CHUNK_SIZE as u64;
        let mut tracker =
            ProgressTracker::new(Uuid::new_v4(), RuntimeTransferDirection::Receive, size, 2);
        tracker.init_files_with_resume(
            &[
                FileDesc {
                    file_id: 1,
                    name: ".gitkeep".to_string(),
                    size: 0,
                },
                FileDesc {
                    file_id: 2,
                    name: "payload.bin".to_string(),
                    size,
                },
            ],
            &HashMap::new(),
        );

        // 空文件初始即完成，不占「还没收齐」的名额
        assert_eq!(tracker.completed_files, 1);
        // 唯一有字节的文件收齐 ⇒ 会话收齐 ⇒ 强制发帧
        assert!(tracker.update_file_chunk(2, size));
    }

    #[test]
    fn resumed_progress_should_restore_completed_file_state() {
        let mut resume_state = HashMap::new();
        resume_state.insert(7, (1, 5));

        let mut tracker =
            ProgressTracker::new(Uuid::new_v4(), RuntimeTransferDirection::Receive, 5, 1);
        tracker.init_files_with_resume(
            &[FileDesc {
                file_id: 7,
                name: "resume.bin".to_string(),
                size: 5,
            }],
            &resume_state,
        );

        let event = tracker
            .progress_event(false)
            .expect("first event is emitted");
        assert_eq!(event.transferred_bytes, 5);
        assert_eq!(event.completed_files, 1);
        assert_eq!(event.files[0].status, FileTransferStatus::Completed);
    }
}
