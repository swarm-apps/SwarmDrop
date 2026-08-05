// 空态与加载态的共用原语。对应桌面 `src/components/layout/section-primitives.tsx` 里的
// `CenteredEmptyState` / `RailEmptyHint`——**两份实现、同一套标准**（同 `master-detail.tsx`
// 与桌面 `MasterDetailShell` 的关系）。
//
// ## 空态要教学，而且要占满可用高度
//
// 此前每个页面的空态都是「一张薄卡 + 一行灰字」，然后下面是大片空白——既没有图标锚点，
// 也没有一个可以点下去的出口，用户只能自己去别处找。impeccable 的 product register 那条
// 「Empty states that teach the interface, not "nothing here"」说的就是这件事。
//
// ## rail 与详情的分工是刻意的
//
// 宽屏下列表与详情同时可见，两侧各摆一套「图标 + 标题 + 描述」就是同一句话说两遍；
// 窄屏用户落在**详情**侧（列表收进抽屉），所以「怎么让它变得非空」这类教学一律放详情，
// rail 只负责确认「这里确实是空的」。
//
// ## 「还没有内容」与「还没加载出来」必须分开
//
// 这套页面此前只看 `items.length === 0`，而节点要等 wasm 拉完 `_bg.wasm` 才第一次 tick——
// 于是**每次刷新，老用户都先看到一个带教学文案的确定性空态**，断言着一件当时并不成立的事。
// [`PanelSkeleton`] 是那一段时间该出现的东西。

import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

/**
 * 居中空态：圆形图标徽章 + 标题 + 说明 + 可选主动作。
 *
 * 默认 `flex-1` 占满父级剩余高度并垂直居中——它要求父级是个有确定高度的 flex 列
 * （`PageShell` 的两个变体都满足）。
 */
export function CenteredEmptyState({
  icon: Icon,
  title,
  description,
  action,
  fill = true,
  iconClassName,
  className,
}: {
  icon: LucideIcon;
  title: ReactNode;
  description?: ReactNode;
  /** 主出口。空态没有出口就只是一句通知，用户还得自己去找路。 */
  action?: ReactNode;
  /**
   * 撑满父级剩余高度并垂直居中（默认）。
   *
   * **只有「这一栏此刻的全部内容就是这个空态」才该撑满**——主从布局的详情栏属于这类：
   * 那一栏本来就是满高的，空态缩起来只会在下面留一片无主的空白。
   *
   * `fill={false}` 用于**面板里的一段**（设备网格空时的那一段）。此前没有这个开关，
   * 于是设备页的空态在一块 320px 的面板里居中，看起来像还没加载完的骨架；
   * 一屏三块都这样，整页就是几个等大的空腔在堆叠。
   */
  fill?: boolean;
  /** 加到图标上的额外类。目前唯一用途是给「正在启动」那颗 `Loader2` 加 `animate-spin`
   *  ——一个不转的加载图标是在说反话。 */
  iconClassName?: string;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3 px-6 text-center",
        fill ? "min-h-0 flex-1 py-12" : "py-8",
        className,
      )}
    >
      <div
        className={cn(
          "flex items-center justify-center rounded-full bg-muted",
          fill ? "size-14" : "size-12",
        )}
      >
        <Icon
          className={cn("text-muted-foreground", fill ? "size-7" : "size-6", iconClassName)}
          aria-hidden
        />
      </div>
      <div className="flex flex-col gap-1">
        <p className="text-sm font-medium text-foreground">{title}</p>
        {/* `text-pretty` 收掉孤字行：这些文案两三行长，末行只剩「进去。」两个字时
            读起来像被截断了。 */}
        {description && (
          <p className="mx-auto max-w-sm text-xs leading-5 text-pretty text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {action && <div className="mt-1">{action}</div>}
    </div>
  );
}

/** 列表栏（rail）的空态：只有一行字。教学归详情侧，见文件头。 */
export function RailEmptyHint({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <p className={cn("px-2 py-8 text-center text-xs text-muted-foreground", className)}>
      {children}
    </p>
  );
}

/**
 * 加载中的骨架。**不是居中 spinner**：骨架保持内容的形状，切换到真内容时不跳版，
 * 也不会让用户盯着一个不知道要等多久的转圈（product register 的
 * "Skeleton states for loading, not spinners in the middle of content"）。
 */
export function PanelSkeleton({ rows = 3, className }: { rows?: number; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-2", className)} aria-hidden>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex flex-col gap-2 rounded-lg border bg-card p-3">
          <div className="h-3 w-1/3 animate-pulse rounded-full bg-muted" />
          <div className="h-2.5 w-2/3 animate-pulse rounded-full bg-muted" />
        </div>
      ))}
    </div>
  );
}
