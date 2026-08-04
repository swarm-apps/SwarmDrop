// track + fill 两层结构的进度条。发送面板（prepare 阶段 hash 进度）与传输活动面板
// （会话字节进度）此前各写了一份逐字节相同的实现。
//
// **它是 `role="progressbar"`，不是一条装饰性色带**：百分比数字在视觉上就在旁边，但读屏
// 用户拿到的只有那串文字，进度条本身此前完全不在无障碍树里。语义补上之后它还能被辅助技术
// 播报变化，而这正是「状态诚实可见」在读屏下的等价物。
//
// `label` 不是可选装饰：`role="progressbar"` 没有可访问名时读屏只会念出一个孤零零的百分比，
// 说不出那是哪一条的进度——一屏里可以同时有会话总进度与逐文件进度好几条。
//
// ## `label={null}`：放在**可交互控件内部**时必须走这条
//
// ARIA 对 `button` 规定 Children Presentational: True —— 它的后代角色会被辅助技术整个丢弃。
// 所以嵌在按钮里的 `role="progressbar"` 不是「弱一点」，是**根本不存在**：既不播报变化，
// 也不被当成进度条。而 `aria-label` 写在那儿会让维护者以为它生效了，这比没写更糟。
//
// 那种位置传 `label={null}`：本组件退成纯装饰（`aria-hidden`），进度信息由**按钮自己的
// 可访问名**承担——名字由后代文本算出，而百分比数字本来就在旁边那行里，读屏照样听得到。
// 换句话说传 null 不是放弃无障碍，是把它交给唯一真正生效的那一层。

import { cn } from "@/lib/cn";

export function ProgressBar({
  percent,
  className,
  label,
}: {
  percent: number;
  className?: string;
  /**
   * 可访问名（如「传输进度」「file.zip 的进度」）。
   *
   * 传 `null` 表示**它在一个可交互控件内部**，语义由那个控件承担——见文件头说明。
   * 做成必填是刻意的：这个取舍必须在每个调用点被想一遍，默认值会让人不假思索地漏掉。
   */
  label: string | null;
}) {
  return (
    <div
      role={label === null ? undefined : "progressbar"}
      aria-hidden={label === null ? true : undefined}
      aria-valuenow={label === null ? undefined : percent}
      aria-valuemin={label === null ? undefined : 0}
      aria-valuemax={label === null ? undefined : 100}
      aria-label={label ?? undefined}
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
