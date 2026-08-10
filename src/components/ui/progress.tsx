import * as React from "react"
import { Progress as ProgressPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

/**
 * 进度所处的阶段。**颜色是这两者之间唯一的视觉差别，别再共用一种**：
 *
 * - `transfer` —— 数据真的在网上跑，品牌青绿；
 * - `local` —— 纯本机阶段（准备 / 校验 / 打包），中性灰，还没上网。
 *
 * 语义的事实源是 DESIGN.md 的 `Transfer Progress Contract`——三端共用那一份判据，只有机制
 * 各不相同（Web `docs/app/app/_components/progress-bar.tsx` 的同名 `tone`、移动端因轨道
 * 本就中性而只换填充）。改这里的取值或加第三种阶段前先改那里，否则三份实现会各自漂移。
 */
type ProgressTone = "transfer" | "local"

/**
 * 轨道 + 填充成对查表。
 *
 * **成对是硬要求**：轨道自带品牌色底纹（`bg-primary/20`），只换填充会得到
 * 「灰填充 + 品牌色底槽」这种比原样更难看的组合。
 *
 * **查表而不是在调用点手写类名**：将来加一种阶段（如「校验中」）会在这里编译期报缺项，
 * 而不是变成某个页面忘了改的颜色——这条是 `Transfer Progress Contract` 明写的要求。
 *
 * 灰档的填充用满不透明度而不是 `/60`：后者与自己的轨道在亮色下只有 2.0:1，够不上
 * WCAG 2.2 SC 1.4.11 对非文本组件的 3:1（满不透明度是 3.7:1 / 暗色 5.0:1）。
 */
const TONE_CLASSES: Record<ProgressTone, { track: string; indicator: string }> =
  {
    transfer: { track: "bg-primary/20", indicator: "bg-primary" },
    local: {
      track: "bg-muted-foreground/20",
      indicator: "bg-muted-foreground",
    },
  }

/**
 * 进度条。`className` 落在轨道上，配色整条由 `tone` 决定。
 *
 * **不要在调用点用 `[&>div]:bg-…` 之类的任意变体改填充色。** 这里此前写着「填充条的
 * `bg-primary` 只能被写进 Indicator 的类名盖掉」——那条机制断言是错的，`[&>div]:bg-x`
 * 的特异性 0,1,1 本来就压得过 `.bg-primary` 的 0,1,0。真正的理由是上面 `TONE_CLASSES`
 * 的两条：轨道与填充必须成对换，且换法要收口成具名阶段而不是散落在调用点。
 *
 * 无障碍：Root 由 Radix 渲染成 `role="progressbar"`，但**可访问名要调用点自己给**
 * （`aria-label`）——一屏里可以同时有好几条，只念「进度条」说不出是哪条的。
 *
 * ⚠️ `value` 只喂给填充条的 `translateX`，**没有转发给 Root**（shadcn 原样如此），
 * 于是 Radix 判它 indeterminate、不输出 `aria-valuenow`。要补先得确认全部调用点的
 * 百分比都夹在 0~100：`settings/-about-section.tsx` 与三个更新对话框是裸
 * `Math.round(x * 100)`，越界会被 Radix `console.error` 打回 indeterminate。
 * 不是一行改动，别顺手加。
 */
function Progress({
  className,
  tone = "transfer",
  value,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root> & {
  /**
   * 见 {@link ProgressTone}。默认 `transfer`——它既是绝大多数调用点，也与加这个
   * 参数之前的外观逐像素相同；**任何本机阶段都必须显式传 `local`**。
   */
  tone?: ProgressTone
}) {
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      className={cn(
        "relative h-2 w-full overflow-hidden rounded-full",
        TONE_CLASSES[tone].track,
        className
      )}
      {...props}
    >
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className={cn(
          "h-full w-full flex-1 transition-all",
          TONE_CLASSES[tone].indicator
        )}
        style={{ transform: `translateX(-${100 - (value || 0)}%)` }}
      />
    </ProgressPrimitive.Root>
  )
}

export { Progress }
