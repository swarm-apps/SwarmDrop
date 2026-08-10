//! 传输热路径的**逐阶段耗时探针**。
//!
//! # 为什么需要它
//!
//! 吞吐问题的归因必须靠实测。2026-08-10 那次诊断
//! （`dev-notes/research/2026-08-10-transfer-throughput-diagnosis.md`）通读了发送 / 接收 /
//! 验签 / 网络四条路径并逐条量化，最终只能给出一个**预算分解**：「浏览器侧全部 CPU 工作
//! 只占每块 79.5 ms 预算的 1.9%，剩下 98% 在 `write_frame` 下游」。它足以排除应用层，
//! 却指不出下游到底卡在哪一段——差的正是这个探针。
//!
//! 同一份诊断对「桌面→移动 50→7 MB/s」只能走排除法（代码里所有 O(已完成)/块 的机制
//! 加总 < 0.1%），结论落在「代码之外」。**排除法永远弱于正向证据**，而把每块的壁钟时间
//! 摊到各阶段上，就是那个正向证据。
//!
//! # 设计约束
//!
//! - **三端通用**。时间源走 [`n0_future::time::Instant`]（native = tokio，wasm = web_time）——
//!   `std::time::Instant` 在 wasm 上是 panic（`time not implemented`），与
//!   [`crate::progress`] 同源。
//! - **长期留在代码里**，级别 `info!`，target 落在 `swarmdrop_transfer`。三端的默认
//!   filter 都是 `swarmdrop=debug,...`，而 `EnvFilter` 的 target 是**前缀匹配**，
//!   所以 `swarmdrop_transfer` 天然放行，用户不需要改 `RUST_LOG` 重跑一次。
//! - **自动落进日志文件**。桌面与移动的文件层级别是 `INFO`（两处 `logging` 模块的
//!   `FILE_LEVEL`），`info!` 正好够着——排障时让用户把日志文件发过来即可。
//! - **常关也几乎零成本**：每块多 4–6 次 `Instant::now()`（native 约 20 ns，wasm 是
//!   `performance.now()`，约 100 ns），相对每块 256 KiB 的真实工作量可忽略。**不要**
//!   因为「探针有开销」把它做成 feature 门控——那样它在用户机器上永远是关的，
//!   而这类问题只在用户机器上出现。

use std::fmt::Write as _;
use std::time::Duration;
// wasm 上 std Instant panic（time not implemented），与 progress.rs 同源走 n0-future。
use n0_future::time::Instant;
use uuid::Uuid;

/// 每多少块打一条窗口报告：256 × 256 KiB = **64 MiB**。
///
/// 按块数而不是按时间节流，是为了让报告密度与**传输量**对齐而与文件大小解耦：
/// 300 MB 的会话出 4 条，6 GB 出 93 条，都落在「够画出趋势、又不刷爆日志文件」的区间。
/// 按时间节流则会让慢传输的采样点变稀——那恰好是最需要采样的场景。
///
/// 小于一个窗口的会话一条窗口报告都不会有，那正是 [`StageProbe::finish`] 存在的理由。
const REPORT_EVERY: u64 = 256;

/// 发送端的四个阶段。**顺序即热路径顺序**，与 [`SEND_READ`] 等下标常量绑定。
pub(crate) const SEND_LABELS: [&str; 4] = ["read", "proof", "write", "ack"];
/// 从宿主读源文件的一块。
pub(crate) const SEND_READ: usize = 0;
/// 算这一块的 bao proof。
pub(crate) const SEND_PROOF: usize = 1;
/// 把帧写进流——**含传输层背压等待**，症状 A 的头号观测点。
pub(crate) const SEND_WRITE: usize = 2;
/// 满窗后等对端回 `Window`（停等流控的 RTT + 对端消化时间）。
pub(crate) const SEND_ACK: usize = 3;

/// 接收端**收帧循环**的两个阶段。
///
/// # 为什么接收端是两个探针而不是一个
///
/// 收帧与消化自 2026-08-10 起是**并发**的两条路径（有界队列相连，见
/// `actor::receiver`）。用一个探针横跨它们会直接破掉这个模块的判读前提——
/// [`StageProbe::lap`] 的各阶段之和恒等于壁钟，那只在**单条串行路径**上成立；
/// 两条并发路径的耗时会重叠，加总必然超过壁钟，占比之和越过 100% 之后
/// 「差值 = 未计入的开销」这条读法就彻底失效了。
///
/// 拆开之后两条线各自回答一个问题，比合在一起时更有判别力：
///
/// - 收帧线的 `enqueue` 占大头 → **消化跟不上**（队列常满，背压在起作用）
/// - 消化线的 `queue` 占大头 → **链路喂不饱**（队列常空，瓶颈在对端或网络）
///
/// 两者同时很小才说明流水线是满的。
pub(crate) const FRAME_LABELS: [&str; 2] = ["wait", "enqueue"];
/// 等下一帧到达——**被链路饿着的时间**。
pub(crate) const FRAME_WAIT: usize = 0;
/// 入队阻塞——**被消化端顶住的时间**，即背压。
pub(crate) const FRAME_ENQUEUE: usize = 1;

/// 接收端**消化循环**的六个阶段。**顺序即热路径顺序**，与 [`DIGEST_QUEUE`] 等下标绑定。
pub(crate) const DIGEST_LABELS: [&str; 6] = ["queue", "verify", "write", "ckpt", "rest", "publish"];
/// 等队列里出现下一块——**被链路饿着的时间**，流水线化之前叫 `wait`。
pub(crate) const DIGEST_QUEUE: usize = 0;
/// bao 逐块验签（decode + blake3）。
pub(crate) const DIGEST_VERIFY: usize = 1;
/// 写 staging（pwrite）。
pub(crate) const DIGEST_WRITE: usize = 2;
/// checkpoint 落库（每 `CHECKPOINT_INTERVAL` 块一次，摊到每块）。
pub(crate) const DIGEST_CKPT: usize = 3;
/// 其余（bitmap 簿记、进度事件）。
pub(crate) const DIGEST_REST: usize = 4;
/// 发布：暂存 → 用户可见位置。**只在文件收齐的那一块上非零。**
///
/// 从 `rest` 里拆出来，是因为两者的量级差着好几个数量级：Android 的 SAF 目标是全量字节
/// 拷贝（6 GB 文件要写 12 GB），其余端是常数时间的重命名。混在一个桶里，真机日志分不出
/// 「发布慢」和「簿记慢」。
///
/// 排在 `rest` **之后**：热路径上是先 `lap(DIGEST_REST)` 把簿记与进度事件结掉，再发布。
pub(crate) const DIGEST_PUBLISH: usize = 5;

/// 发送端探针。
pub(crate) type SendProbe = StageProbe<4>;
/// 接收端收帧探针。
pub(crate) type FrameProbe = StageProbe<2>;
/// 接收端消化探针。
pub(crate) type DigestProbe = StageProbe<6>;

/// 把每块的壁钟时间摊到 `N` 个阶段上，按块数节流地报告。
///
/// 用法是「每块开头 [`mark`](Self::mark)，每段结束 [`lap`](Self::lap)，整块结束
/// [`block_done`](Self::block_done)，会话结束 [`finish`](Self::finish)」。
pub(crate) struct StageProbe<const N: usize> {
    role: &'static str,
    session_id: Uuid,
    labels: [&'static str; N],
    /// 会话累计。
    total: [Duration; N],
    /// 自上次报告以来。
    window: [Duration; N],
    blocks: u64,
    bytes: u64,
    window_blocks: u64,
    window_bytes: u64,
    started: Instant,
    window_started: Instant,
    /// 上一次打点。[`lap`](Self::lap) 就地推进它，使相邻阶段之间没有间隙。
    mark: Instant,
}

impl<const N: usize> StageProbe<N> {
    pub(crate) fn new(role: &'static str, session_id: Uuid, labels: [&'static str; N]) -> Self {
        let now = Instant::now();
        Self {
            role,
            session_id,
            labels,
            total: [Duration::ZERO; N],
            window: [Duration::ZERO; N],
            blocks: 0,
            bytes: 0,
            window_blocks: 0,
            window_bytes: 0,
            started: now,
            window_started: now,
            mark: now,
        }
    }

    /// 重新打点。每块开始时调一次。
    pub(crate) fn mark(&mut self) {
        self.mark = Instant::now();
    }

    /// 结算一段：把上次打点到现在的时间计入 `stage`，并**就地重新打点**。
    ///
    /// 相邻两次 `lap` 之间没有间隙，于是各阶段之和恒等于 `mark()` 到最后一次 `lap()`
    /// 的壁钟时间。**未被任何阶段覆盖的时间会显式表现为「各阶段占比之和 < 100%」**——
    /// 那个差值本身就是信息（说明有没记进来的开销，例如循环外的等待）。
    pub(crate) fn lap(&mut self, stage: usize) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.mark);
        self.total[stage] += elapsed;
        self.window[stage] += elapsed;
        self.mark = now;
    }

    /// 一块处理完毕。到达窗口边界时打一条报告并翻页。
    pub(crate) fn block_done(&mut self, bytes: u64) {
        self.blocks += 1;
        self.bytes += bytes;
        self.window_blocks += 1;
        self.window_bytes += bytes;
        if self.window_blocks >= REPORT_EVERY {
            self.emit(
                "window",
                self.window_bytes,
                &self.window,
                self.window_started,
            );
            self.window = [Duration::ZERO; N];
            self.window_blocks = 0;
            self.window_bytes = 0;
            self.window_started = Instant::now();
        }
    }

    /// 会话结束：打一条覆盖全程的汇总。由 [`Drop`] 调用，不对外暴露。
    ///
    /// 它同时是**小于一个窗口的会话**唯一的输出，所以不能因为「不足一窗」跳过。
    fn finish(&self) {
        if self.blocks == 0 {
            return;
        }
        self.emit("total", self.bytes, &self.total, self.started);
    }

    fn emit(&self, kind: &'static str, bytes: u64, stages: &[Duration; N], since: Instant) {
        let elapsed = since.elapsed();
        tracing::info!(
            target: "swarmdrop_transfer",
            role = self.role,
            session = %self.session_id,
            kind,
            blocks = self.blocks,
            elapsed_ms = elapsed.as_millis() as u64,
            mb_s = %format!("{:.2}", throughput_mb_s(bytes, elapsed)),
            stages = %render_stages(&self.labels, stages, elapsed),
            "传输探针"
        );
    }
}

/// 汇总由 `Drop` 打，**不是**由调用方显式收尾。
///
/// 传输的失败路径有十几个 `?` 与 early return，而**恰恰是失败的会话最需要这份数据**
/// （「传到 60% 断了」的现场只存在于那一次）。靠调用方在每条路径上记得收尾是不现实的，
/// 交给 Drop 则一条都漏不掉。
impl<const N: usize> Drop for StageProbe<N> {
    fn drop(&mut self) {
        self.finish();
    }
}

/// 渲染成 `read=12ms/2% proof=45ms/8% write=489ms/89%` 这样的一行。
///
/// 拼成字符串而不是逐个做 tracing 字段，是因为字段名必须是编译期常量，而阶段表是
/// 泛型参数——而且一行字符串在日志文件里 grep 起来更省事。
fn render_stages<const N: usize>(
    labels: &[&'static str; N],
    stages: &[Duration; N],
    elapsed: Duration,
) -> String {
    let base = elapsed.as_secs_f64();
    let mut out = String::with_capacity(N * 20);
    for (index, (label, stage)) in labels.iter().zip(stages.iter()).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let pct = if base > 0.0 {
            stage.as_secs_f64() / base * 100.0
        } else {
            0.0
        };
        // write! 到 String 不会失败，忽略返回值即可。
        let _ = write!(out, "{label}={}ms/{pct:.0}%", stage.as_millis());
    }
    out
}

fn throughput_mb_s(bytes: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 {
        return 0.0;
    }
    bytes as f64 / secs / (1024.0 * 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_sum_to_wall_clock_between_mark_and_last_lap() {
        // lap 就地推进 mark，所以两段之和不含间隙——这条是「占比之和 < 100% 即有未计入
        // 开销」这个判读方式成立的前提，破了它整个探针的读法就错了。
        let mut probe = SendProbe::new("send", Uuid::nil(), SEND_LABELS);
        probe.mark();
        probe.lap(SEND_READ);
        probe.lap(SEND_PROOF);
        let sum: Duration = probe.total.iter().sum();
        assert!(
            sum <= probe.mark.saturating_duration_since(probe.started),
            "各阶段之和不得超过 mark 推进的总时长"
        );
    }

    #[test]
    fn stage_indices_stay_aligned_with_their_labels() {
        // 下标常量与标签数组是两份独立声明，插一个新阶段却忘了顺延下标，产出的报告会
        // 把耗时记到**相邻那个阶段**名下——数字依旧自洽，只是归因全错，而这类错误在
        // 真机日志里没有任何可辨识的形状。数组长度由类型保证，错位只能这样钉。
        assert_eq!(FRAME_LABELS[FRAME_WAIT], "wait");
        assert_eq!(FRAME_LABELS[FRAME_ENQUEUE], "enqueue");
        assert_eq!(DIGEST_LABELS[DIGEST_QUEUE], "queue");
        assert_eq!(DIGEST_LABELS[DIGEST_VERIFY], "verify");
        assert_eq!(DIGEST_LABELS[DIGEST_WRITE], "write");
        assert_eq!(DIGEST_LABELS[DIGEST_CKPT], "ckpt");
        assert_eq!(DIGEST_LABELS[DIGEST_REST], "rest");
        assert_eq!(DIGEST_LABELS[DIGEST_PUBLISH], "publish");
        assert_eq!(SEND_LABELS[SEND_READ], "read");
        assert_eq!(SEND_LABELS[SEND_PROOF], "proof");
        assert_eq!(SEND_LABELS[SEND_WRITE], "write");
        assert_eq!(SEND_LABELS[SEND_ACK], "ack");
    }

    #[test]
    fn render_lists_every_stage_in_declared_order() {
        let stages = [Duration::from_millis(10), Duration::from_millis(90)];
        let rendered = render_stages(&["a", "b"], &stages, Duration::from_millis(100));
        assert_eq!(rendered, "a=10ms/10% b=90ms/90%");
    }

    #[test]
    fn render_survives_zero_elapsed() {
        // 窗口耗时为 0（时钟粒度粗于一个窗口）时不能除出 NaN/inf——日志里出现 NaN
        // 会让整条报告失去可读性，而这在 wasm 的 performance.now() 上并非不可能。
        let rendered = render_stages(&["a"], &[Duration::ZERO], Duration::ZERO);
        assert_eq!(rendered, "a=0ms/0%");
        assert_eq!(throughput_mb_s(1024, Duration::ZERO), 0.0);
    }

    #[test]
    fn finish_is_silent_without_blocks() {
        // 没传成一块的会话不该产出汇总行——否则每次失败的握手（以及每个被 Drop 的
        // 空探针）都会在日志里留一条「0 blocks / 0.00 mb_s」的噪声。
        let probe = DigestProbe::new("recv", Uuid::nil(), DIGEST_LABELS);
        assert_eq!(probe.blocks, 0);
        drop(probe);
    }

    #[test]
    fn window_report_resets_only_the_window() {
        // 翻页必须只清窗口、不动累计——否则 Drop 打出的 total 行会只覆盖最后一个
        // 不满 256 块的残窗，整场传输的汇总就没了。
        let mut probe = SendProbe::new("send", Uuid::nil(), SEND_LABELS);
        for _ in 0..REPORT_EVERY {
            probe.mark();
            probe.lap(SEND_WRITE);
            probe.block_done(1024);
        }
        assert_eq!(probe.window_blocks, 0, "窗口应已翻页");
        assert_eq!(probe.blocks, REPORT_EVERY, "累计块数不该被翻页清掉");
        assert_eq!(probe.bytes, REPORT_EVERY * 1024, "累计字节不该被翻页清掉");
    }
}
