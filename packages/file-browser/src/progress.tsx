"use client";

import { cn } from "./cn";

/**
 * 进度所处的阶段。
 *
 * - `transfer` —— 数据在网上跑，品牌青绿；
 * - `paused` —— 传输已开始但停住了，琥珀警示色。
 *
 * **暂停必须换色，不能只靠图标。** 这里此前只画 `bg-primary`、暂停仅由行内一个
 * 中性灰的 `<Pause />` 表达，而移动端同一场景是琥珀色进度条 + 琥珀色 `CirclePause`
 * ——同一个状态在两端一个「不显眼的灰」一个「需要注意的黄」。暂停是**用户可恢复**的
 * 中断，在一列文件里要能一眼扫见；中性灰让它和「等待中」看起来一样。三端已统一到
 * 琥珀（2026-08-10）。
 *
 * 与 `Transfer Progress Contract` 的 `transfer` / `local` 是**正交**的两组：那组分
 * 「本机准备 vs 真在传」，这组分「在传 vs 停住了」。本包不画本机准备阶段，故不含 `local`。
 */
export type ProgressTone = "transfer" | "paused";

/**
 * 轨道 + 填充成对查表——只换填充会得到「琥珀填充 + 品牌色底槽」。
 *
 * 查表而不是让调用点传类名：加一种阶段会在这里编译期报缺项，而不是变成某个页面忘了
 * 改的颜色。判据见根目录 `DESIGN.md` 的 `Transfer Progress Contract`。
 */
const TONE_CLASSES: Record<ProgressTone, { track: string; fill: string }> = {
  transfer: { track: "bg-primary/20", fill: "bg-primary" },
  // 填充是 `warning-ink` 不是 `warning`：琥珀原色在**亮色**下与自己 20% 的轨道只有
  // 1.83:1，够不上 WCAG 2.2 SC 1.4.11 对非文本组件的 3:1（暗色 6.03:1 没问题，只看
  // 暗色会以为没事）。ink 变体是 4.33:1 / 5.45:1。这与 `src/index.css` 那条既定纪律
  // 同源——状态色原色只配当大色块，一旦要跟浅底分清就得用 ink。
  paused: { track: "bg-warning/20", fill: "bg-warning-ink" },
};

/**
 * 进度条。**刻意不依赖 radix**——桌面的 `ui/progress.tsx` 走 `radix-ui`，Web 有自己的
 * `ProgressBar`，两者都只是「一条轨道 + 一段填充」。为这点结构引入一个带模块级状态的
 * 依赖，等于给本包多开一个实例分裂的口子（见 README 的 `file:` 那节）。
 *
 * `value` 为 `null` 时渲染成不确定态的空轨道（而不是 0%）——「还不知道进度」与
 * 「进度是 0」是两件事。
 */
export function Progress({
  value,
  className,
  label,
  tone = "transfer",
}: {
  value: number | null;
  className?: string;
  /**
   * 可访问名。传 `null` 表示**装饰性**：调用方已经在祖先元素上表达了进度
   * （例如整行是个 `<button>`，其可访问名里就带着百分比）。
   * ARIA 会丢弃交互控件的后代角色，那时再挂一个 progressbar 只是噪声。
   */
  label: string | null;
  /** 进度所处阶段，决定配色。默认 `transfer`。 */
  tone?: ProgressTone;
}) {
  const percent = value === null ? null : Math.min(100, Math.max(0, value));
  const { track, fill } = TONE_CLASSES[tone];

  return (
    <div
      className={cn(
        "relative h-1.5 w-full overflow-hidden rounded-full",
        track,
        className,
      )}
      {...(label === null
        ? { "aria-hidden": true }
        : {
            role: "progressbar",
            "aria-label": label,
            "aria-valuemin": 0,
            "aria-valuemax": 100,
            ...(percent === null ? {} : { "aria-valuenow": Math.round(percent) }),
          })}
    >
      <div
        className={cn(
          "h-full rounded-full transition-[width] duration-300 ease-out",
          fill,
        )}
        style={{ width: `${percent ?? 0}%` }}
      />
    </div>
  );
}
