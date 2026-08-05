// 区块外壳与区块标题。对应桌面 `src/components/layout/section-primitives.tsx`——
// 两份实现、同一套标准。
//
// ## 玻璃只给结构容器（DESIGN.md 的 Flat-Control, Glass-Chrome Rule）
//
// 「你点它、往里打字」的东西一律扁平（`shadow-xs` 而已）；「装着别的东西」的东西才上玻璃。
// 所以：页面级面板 = [`SectionShell`]（`glass-panel` + 24px 圆角），面板里的设备卡 / 列表行 /
// 文件行一律 `bg-card` + 1px 边框。**永远不要给 button / input 加 `backdrop-filter`。**
//
// 圆角是两套词汇不是一套：控件 6–14px、面板 18–24px。Web 应用区此前全塌成 `rounded-lg`
// 与 `rounded-xl`，面板与按钮的圆角一样大，于是层级只能靠边框颜色暗示。
//
// ## 为什么 Web 上没有 WebGL 极光背景
//
// 桌面的玻璃是浮在一层缓慢流动的 aurora 上的，那层背景承担了大部分「活着的网络」的观感。
// Web 端刻意不搬它：`ogl` 是 +30KB 且持续占 GPU，而这里的基线视口是手机浏览器、
// 典型会话是「打开标签页快速收发一次文件」。玻璃本身（纯 CSS、零依赖）保留，
// 因为它承担的是**层级**而不是氛围。

import { ChevronDown, type LucideIcon } from "lucide-react";
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/cn";

/**
 * 主从两栏（列表栏 / 详情栏）共用的**面板皮**：18px 圆角 + 1px 边框 + 卡底 + 一层浅投影。
 *
 * 只收敛「皮」，布局（flex 方向、min-h-0、怎么滚、内边距）留在调用点——因为那部分**本来
 * 就各不相同**：列表栏 `overflow-hidden`（内部自己有滚动区），详情栏 `overflow-y-auto`
 * 且带 24px 内边距。把差异也塞进一个吃 variant 的组件，读的人反而要跳一次才知道这一栏怎么滚。
 *
 * 与 [`SectionShell`] 的分工：那个是**页面级**面板（玻璃 + 24px 圆角），用在竖向内容页；
 * 这个是主从布局里的栏，不上玻璃（它装满了列表行，玻璃会和行的边框打架）。
 */
export const PANEL_SURFACE = "rounded-[var(--radius-panel-sm)] border bg-card shadow-xs";

/**
 * 主从列表里「这一行是当前选中项」的表达（品牌色边框 + accent 底 + 一圈淡 ring）。
 *
 * **两个分支一起收**，不是只收选中态：未选中态那句 `hover:bg-accent` 与选中态是一对
 * ——只搬一半，两边的悬停反馈迟早会各改各的，而它们是同一个列表在两条路由上的两种内容。
 * 收件箱行与传输会话行本来就该长得一样。
 */
export function selectedRowClass(selected: boolean): string {
  return selected
    ? "border-[var(--brand)]/40 bg-accent ring-1 ring-[var(--brand)]/20"
    : "hover:bg-accent";
}

/** 页面级面板外壳：玻璃 + 24px 圆角。 */
export function SectionShell({
  children,
  className,
  ...props
}: { children: ReactNode; className?: string } & HTMLAttributes<HTMLElement>) {
  return (
    <section
      {...props}
      className={cn(
        // 面板内分区 16px、面板内边距 20px。**两个数不能相等**——相等时内容离边缘和
        // 内容离彼此一样远，面板就不再像一个「装着东西的容器」，而像一张网格纸。
        // 内边距由容器角色决定（面板 20 / 卡片 14），不由内容决定，见 global.css 的间距语义。
        "glass-panel flex flex-col gap-[var(--space-in-panel)] rounded-[var(--radius-panel)] p-[var(--space-panel)]",
        className,
      )}
    >
      {children}
    </section>
  );
}

/**
 * 区块标题：图标片 + 标题 + 可选描述 + 右侧计数。
 *
 * 计数不是装饰：设备网格此前直接就是一个 `<ul>`，「已配对设备」这个概念只在空态文案里
 * 出现过一次——非空时用户看到的是一堆卡片，没有任何东西说它们是什么、有几台。
 */
export function SectionHeader({
  title,
  count,
  icon: Icon,
  description,
  action,
  disclosure,
}: {
  title: ReactNode;
  count?: number;
  icon?: LucideIcon;
  description?: ReactNode;
  /** 右上角的区块级动作（如「管理分组」）。 */
  action?: ReactNode;
  /**
   * 让整个标题块变成展开/收起开关。用于**使用频率远低于同页其它区块**的面板
   * （配对：一次性动作，配完即长期信任），收起后它只占一行而不是半屏。
   *
   * 形态是 ARIA APG 的手风琴：**button 在 `<h2>` 里面**，不是反过来。反过来写
   * （`<button><h2>…</h2></button>`）是无效 HTML——button 的内容模型是 phrasing content，
   * 标题是 flow content——而且读屏会丢掉这一层的标题语义。
   */
  disclosure?: { open: boolean; onToggle: () => void; controls: string };
}) {
  // 包着图标的小片是「装东西的容器」，所以它也吃玻璃——同桌面。
  const iconChip = Icon ? (
    <span className="glass-control mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full text-brand">
      <Icon className="size-3.5" aria-hidden />
    </span>
  ) : null;

  return (
    <div className="flex min-w-0 items-start justify-between gap-3">
      {disclosure ? (
        <div className="flex min-w-0 flex-1 gap-2.5">
          {iconChip}
          <div className="min-w-0 flex-1">
            {/*
              **button 在 `<h2>` 里面**（ARIA APG 的手风琴形态），且**只包标题**。
              把描述也包进去，标题的可访问名就变成「配对 与另一台设备互换一次邀请即可，
              之后长期有效。」——读屏按标题导航时会读到一整段。描述留在按钮外面。
            */}
            <h2>
              <button
                type="button"
                aria-expanded={disclosure.open}
                // 被控元素是条件渲染的：收起时它不在 DOM 里，这时留着 `aria-controls`
                // 就是一个悬空 IDREF（axe 的 aria-valid-attr-value 会报）。
                aria-controls={disclosure.open ? disclosure.controls : undefined}
                onClick={disclosure.onToggle}
                // `-m-1 p-1`：把命中区往外扩一圈但不改变视觉位置。
                className="focus-ring -m-1 flex w-full min-w-0 items-center gap-2 rounded-lg p-1 text-left transition-colors hover:bg-foreground/[0.03] dark:hover:bg-white/[0.04]"
              >
                <span className="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-tight text-foreground">
                  {title}
                </span>
                <ChevronDown
                  className={cn(
                    "size-4 shrink-0 text-muted-foreground transition-transform duration-200 ease-[cubic-bezier(0.25,1,0.5,1)]",
                    disclosure.open && "rotate-180",
                  )}
                  aria-hidden
                />
              </button>
            </h2>
            {description && (
              <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
            )}
          </div>
        </div>
      ) : (
        <div className="flex min-w-0 gap-2.5">
          {iconChip}
          <div className="min-w-0">
            <h2 className="truncate text-[15px] font-semibold tracking-tight text-foreground">
              {title}
            </h2>
            {description && (
              <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
            )}
          </div>
        </div>
      )}
      <div className="flex shrink-0 items-center gap-2">
        {typeof count === "number" && (
          <span className="rounded-full bg-foreground/[0.045] px-2.5 py-1 text-[11px] font-semibold tabular-nums text-muted-foreground dark:bg-white/[0.06]">
            {count}
          </span>
        )}
        {action}
      </div>
    </div>
  );
}
