/**
 * 活跃传输进度的两条**时效判据**。
 *
 * 它们不是格式化，是「这个数字现在还能不能拿出来给人看」——三端各有一份渲染代码，
 * 判据分家就会变成一端 5 秒、一端 10 秒、一端忘了做。
 */

/**
 * 一帧进度事件的保鲜期（毫秒）。取 6 秒 = 2× 后端速度滑窗（`SPEED_WINDOW = 3s`）。
 *
 * ## 为什么必须由前端来判
 *
 * 后端的 `ProgressTracker::speed()` 已经会在样本老于滑窗时归零，于是**停滞之后的下一帧**
 * 会诚实地带上 `speed: 0` / `eta: null`。问题是**可能根本没有下一帧**：进度事件是从收块
 * 路径上发出来的，传输域里没有任何自走的 tick，所以对端一安静，最后那帧就永远躺在 store 里。
 *
 * 不判时效的话，界面会把一个早已不成立的「剩余 45s」一直显示到会话超时翻挂起为止 ——
 * ETA 不是在传输出问题时消失，而是在传输出问题时**撒谎**，后者更糟。
 *
 * 2× 滑窗这个取值：小于滑窗会与「后端刚要归零」的正常抖动打架，大得多则失去意义。
 */
export const PROGRESS_STALE_MS = 6_000;

/**
 * 发布态延迟多久才显示（毫秒）。
 *
 * 桌面 / Web / iOS 的发布是常数时间（同卷 rename、OPFS close），`started` 与 `finished`
 * 背靠背到达却是**两条独立事件、两次渲染**，急着画就会让进度条每收齐一个文件闪一下灰 ——
 * 收一个几百个小文件的目录时就是持续频闪，正是 Transfer Progress Contract 开篇那个
 * 「视觉突变被读成传输重新开始」的失败样式的高频版。
 *
 * 真正需要解释的发布（Android SAF 全量拷贝）都远长于这个阈值，延迟揭示不会漏掉它们。
 */
export const PUBLISH_VISIBLE_AFTER_MS = 300;

/**
 * 这一帧进度还新鲜吗。`receivedAt` 为空视为陈旧——没记录到达时刻就无从担保。
 */
export function isProgressFresh(
  receivedAt: number | null | undefined,
  now: number,
): boolean {
  if (receivedAt == null || !Number.isFinite(receivedAt)) return false;
  return now - receivedAt < PROGRESS_STALE_MS;
}

/**
 * 取一帧进度里**还能用**的剩余秒数：帧已陈旧就返回 `null`，由调用点显示占位。
 *
 * 与 `formatEta` 分工：这里回答「还算不算数」，那里回答「怎么写出来」。
 */
export function usableEta(
  eta: number | null | undefined,
  receivedAt: number | null | undefined,
  now: number,
): number | null {
  if (!isProgressFresh(receivedAt, now)) return null;
  return eta ?? null;
}

/** 一帧进度里带保质期的两个数。 */
export interface UsableRates {
  /** 剩余秒数；陈旧或算不出为 `null`。 */
  eta: number | null;
  /** 速率（B/s）；陈旧或算不出为 `null`。 */
  speed: number | null;
}

/**
 * **速度与剩余时间必须一起过期。**
 *
 * 两者同源于同一个滑窗，所以「帧太旧」对它们是同一件事。分开判的后果是同一行里一个诚实
 * 一个撒谎——ETA 已经退成「计算中」，旁边还写着 `12.4 MB/s`，比两个都冻住更像 bug。
 * 三端各自记得判两次是靠不住的，所以这里一次给出两个。
 *
 * 已传字节 / 百分比**不在此列**：它们是累计量，不会因为没有新帧而失效，作废它们会让进度条
 * 倒退回一个更旧的值。
 */
export function usableRates(
  frame: { eta?: number | null; speed?: number | null } | null | undefined,
  receivedAt: number | null | undefined,
  now: number,
): UsableRates {
  if (!isProgressFresh(receivedAt, now)) return { eta: null, speed: null };
  return { eta: frame?.eta ?? null, speed: frame?.speed ?? null };
}
