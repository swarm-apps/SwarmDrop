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

import type { LucideIcon } from "lucide-react";
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
        "glass-panel flex flex-col gap-4 rounded-[var(--radius-panel)] p-4",
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
}: {
  title: ReactNode;
  count?: number;
  icon?: LucideIcon;
  description?: ReactNode;
  /** 右上角的区块级动作（如「管理分组」）。 */
  action?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 items-start justify-between gap-3">
      <div className="flex min-w-0 gap-2.5">
        {Icon && (
          // 包着图标的小片是「装东西的容器」，所以它也吃玻璃——同桌面。
          <span className="glass-control mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full text-brand">
            <Icon className="size-3.5" aria-hidden />
          </span>
        )}
        <div className="min-w-0">
          <h2 className="truncate text-[15px] font-semibold tracking-tight text-foreground">
            {title}
          </h2>
          {description && (
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
          )}
        </div>
      </div>
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
