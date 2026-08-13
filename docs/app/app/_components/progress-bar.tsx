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

/**
 * 进度所处的阶段。**颜色是这两者之间唯一的视觉差别，别再共用一种**：
 *
 * - `transfer` —— 数据真的在网上跑，品牌青绿；
 * - `local` —— 纯本机阶段（准备 / 校验 / 打包），中性灰，还没上网。
 *
 * 此前 prepare 与传输同色同形，且背靠背出现（prepare 走完立刻换成传输条从 0 起），
 * 大文件上用户会把 prepare 读成传输、再把新发送读成「续传倒回 0%」。
 *
 * 语义的事实源是 DESIGN.md 的 `Transfer Progress Contract`——三端共用那一份判据，只有机制
 * 各不相同（桌面 `src/components/ui/progress.tsx` 有同名的 `tone`，但轨道自带品牌色，
 * 查的是「轨道 + 填充」成对的表；移动端轨道本就中性，只换填充）。改这里的取值或加第三种
 * 阶段前先改那里，否则三份实现会各自漂移。
 */
type ProgressTone = "transfer" | "local";

/**
 * 查表而不是三元：将来加一种阶段（如「校验中」）会在这里编译期报缺项。
 *
 * 灰档用满不透明度的 `bg-muted-foreground` 而不是 `/60`：填充与轨道（`bg-muted`）的
 * 对比度是这条子唯一能被看出「走到哪了」的线索，`/60` 在亮色下只有 2.2:1，够不上
 * WCAG 2.2 SC 1.4.11 对非文本组件的 3:1；满不透明度是 4.3:1（暗色 6.6:1）。
 * 青绿档同理要守住这条线（亮色 3.7:1）——换 `--brand-solid` 的值时一起复核。
 */
const TONE_FILL: Record<ProgressTone, string> = {
  transfer: "bg-[var(--brand-solid)]",
  local: "bg-muted-foreground",
};

export function ProgressBar({
  percent,
  className,
  tone = "transfer",
  label,
}: {
  percent: number;
  className?: string;
  /**
   * 见 {@link ProgressTone}。默认 `transfer`——它是绝大多数调用点，
   * 但**任何本机阶段都必须显式传 `local`**。
   */
  tone?: ProgressTone;
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
        className={cn(
          "h-full rounded-full transition-[width] duration-300 motion-reduce:transition-none",
          TONE_FILL[tone],
        )}
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}
