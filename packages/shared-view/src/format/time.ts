/**
 * 时间量的展示语言：时长、延迟、剩余有效期。
 *
 * ## 为什么只有「紧凑拉丁形式」进得来
 *
 * 时长有两种写法：紧凑拉丁（`1h 5m`）与本地化词（`1 小时 5 分`）。收口前桌面用前者、
 * Web 用后者，两者都硬编码。
 *
 * 这里取**紧凑拉丁**：它跨语言可读、不需要 i18n 运行时，因此才可能住在这个包里。想要
 * 本地化词的场合走 [`formatTimeLeft`]——它显式收一个 locale，用 `Intl` 生成，是本模块唯一
 * 与语言有关的函数。
 *
 * ## 被判出去的两个
 *
 * `formatUptime`（桌面「2 小时 15 分钟」/ 移动「2h 15m」）与 `formatRelativeTime`
 * （桌面硬编码中文 / 移动返回 `<Trans>` 节点）**不进本包**：它们的输出是各端有意不同的
 * 本地化文案，收进来就必须改掉其中一端的渲染。判据见 README 的「归属判据」第 3 条。
 */

const MINUTE = 60;
const HOUR = 60 * MINUTE;

/**
 * 秒级时长的紧凑形式：`45s` / `3m 20s` / `1h 5m`。
 *
 * 向上取整而非四舍五入——ETA 说「还剩 0 秒」却没结束，比多报一秒难解释得多。
 */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0s";
  if (seconds < MINUTE) return `${Math.ceil(seconds)}s`;
  if (seconds < HOUR) {
    return `${Math.floor(seconds / MINUTE)}m ${Math.ceil(seconds % MINUTE)}s`;
  }
  return `${Math.floor(seconds / HOUR)}h ${Math.ceil((seconds % HOUR) / MINUTE)}m`;
}

/**
 * 连接延迟。
 *
 * **≤0 返回 `null`**：0ms 是取整后的占位（<1ms 的直连），显示出来像 bug。由调用方决定
 * 只显示连接类型、不显示延迟。
 */
export function formatLatency(ms: number | null | undefined): string | null {
  return ms != null && ms > 0 ? `${ms}ms` : null;
}

/**
 * 剩余时长的人类文案，跟随传入的 locale（邀请有效期用）。
 *
 * **按量级切粒度**：邀请 TTL 是 24 小时，`m:ss` 会渲染成「1439:59」这种没人读得懂的东西。
 * 小时级只说小时、分钟级说分钟、最后一分钟才精确到秒——那时用户确实在盯着它。
 *
 * **只返回时长本身，不带方向词**（「23 小时」而非「23 小时后」）：方向词归调用点的翻译
 * （「将在 {0} 后过期」/「Expires in {0}」），否则两边都说一次就成了「将在 23 小时后 后过期」。
 * 所以用 `Intl.NumberFormat` 的 unit 样式，而不是自带方向的 `Intl.RelativeTimeFormat`。
 *
 * **过期返回空串**：「已过期」是一句需要翻译的 UI 文案，调用方应当先判过期再走自己的分支。
 *
 * @param locale BCP 47 标签，由调用方从各自的 i18n 运行时取。
 */
export function formatTimeLeft(seconds: number, locale: string): string {
  if (!(seconds > 0)) return "";

  const format = (value: number, unit: "hour" | "minute" | "second") =>
    new Intl.NumberFormat(locale, { style: "unit", unit, unitDisplay: "long" }).format(value);

  if (seconds < MINUTE) return format(Math.round(seconds), "second");
  const totalMinutes = Math.round(seconds / MINUTE);
  if (totalMinutes < 60) return format(totalMinutes, "minute");
  return format(Math.round(totalMinutes / 60), "hour");
}
