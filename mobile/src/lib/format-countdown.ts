/**
 * **当面配对时那个码面倒计时**的 `mm:ss` 形式。
 *
 * ## 它不是 `formatTimeLeft`，别混
 *
 * `@swarmdrop/shared-view` 里那个同名函数给的是本地化词（「59 分钟」「23 小时」），三端
 * 共用，用于**列表**里的剩余有效期。这里要的是另一件事：两个人举着手机当面扫码时，秒级
 * 跳动是有用的反馈——那个场景下「59 分钟」既不精确也不给人「快没了」的紧迫感。
 *
 * 一度把这个实现叫 `formatTimeLeft` 并从 `invite-exchange.tsx` 提出来「给两处共用」，
 * 结果是同一个「已发出的邀请」组件在手机上显示 `59:03 后失效`、桌面显示「59 分钟后失效」
 * ——那正是提取时想避免的漂移，只不过漂的是**跨端**而不是跨文件。
 *
 * 规矩：**列表里的剩余期用共享包的 `formatTimeLeft`；只有码面倒计时用这个。**
 */
export function formatCountdown(seconds: number): string {
  if (!(seconds > 0)) return "0:00";
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
