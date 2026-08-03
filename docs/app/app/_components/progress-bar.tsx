// track + fill 两层结构的进度条。发送面板（prepare 阶段 hash 进度）与传输活动面板
// （会话字节进度）此前各写了一份逐字节相同的实现。
//
// **它是 `role="progressbar"`，不是一条装饰性色带**：百分比数字在视觉上就在旁边，但读屏
// 用户拿到的只有那串文字，进度条本身此前完全不在无障碍树里。语义补上之后它还能被辅助技术
// 播报变化，而这正是「状态诚实可见」在读屏下的等价物。
//
// `label` 不是可选装饰：`role="progressbar"` 没有可访问名时读屏只会念出一个孤零零的百分比，
// 说不出那是哪一条的进度——一屏里可以同时有会话总进度与逐文件进度好几条。

import { cn } from "@/lib/cn";

export function ProgressBar({
  percent,
  className,
  label,
}: {
  percent: number;
  className?: string;
  /** 可访问名（如「传输进度」「file.zip 的进度」）。 */
  label: string;
}) {
  return (
    <div
      role="progressbar"
      aria-valuenow={percent}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label}
      className={cn("h-1.5 overflow-hidden rounded-full bg-muted", className)}
    >
      <div
        // 时长显式给出：默认的 150ms 在 1s 一帧的进度事件下会走走停停，300ms 刚好把两帧接上。
        // `motion-reduce` 下直接跳变——降级路径与本仓其它动效同一条纪律（PRODUCT.md 无障碍段）。
        className="h-full rounded-full bg-[var(--brand-solid)] transition-[width] duration-300 motion-reduce:transition-none"
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}
