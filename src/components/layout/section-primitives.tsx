/**
 * 通用区块布局原语
 * 玻璃面板外壳、区块标题、内嵌空态面板、居中空态——跨页面（设备 / 收件箱 / 传输）复用。
 */

import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export function SectionHeader({
  title,
  count,
  icon: Icon,
  description,
}: {
  title: React.ReactNode;
  count?: number;
  icon?: React.ComponentType<{ className?: string }>;
  description?: React.ReactNode;
}) {
  return (
    <div className="flex min-w-0 items-start justify-between gap-3">
      <div className="flex min-w-0 gap-2.5">
        {Icon && (
          <span className="glass-control mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground">
            <Icon className="size-3.5" />
          </span>
        )}
        <div className="min-w-0">
          <h2 className="truncate text-[15px] font-semibold tracking-tight text-foreground">
            {title}
          </h2>
          {description && (
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              {description}
            </p>
          )}
        </div>
      </div>
      {typeof count === "number" && (
        <span className="rounded-full bg-foreground/[0.045] px-2.5 py-1 text-[11px] font-semibold text-muted-foreground dark:bg-white/[0.06]">
          {count}
        </span>
      )}
    </div>
  );
}

/**
 * 页面级面板外壳：玻璃 + 24px 圆角。
 *
 * **不要在这里放 `min-h-full`。** 它曾经烤在这个原语里，理由是「让独占一栏的面板把玻璃
 * 铺到栏底」——但那对**同一栏里放两个面板**的场合是错的：`min-height: 100%` 解析的是
 * 父栏高度，于是**每一个**面板都要求整栏那么高。设备页左栏正好是两个
 * （已配对设备 + 活跃传输），实测网格行 1468px、两个面板各 1468 加 20 的 gap = 2956，
 * 溢出 1488px —— `overflow: visible` 让它们一路画到网格的 `py-5` 底部内边距之外、
 * 也画到滚动容器之外，滚到底就是「卡片贴着窗口底边、像漏了 padding」。
 *
 * 需要「铺满整栏」的调用点自己写 `className="flex-1"`：在定高的 flex 列里它表达的是
 * 「分配剩余空间」，两个都写也只是**平分**，不会各自要求 100%。
 */
export function SectionShell({
  children,
  className,
  ...props
}: {
  children: React.ReactNode;
  className?: string;
} & HTMLAttributes<HTMLElement>) {
  return (
    <section
      {...props}
      className={cn("glass-panel flex flex-col gap-4 rounded-[24px] p-4", className)}
    >
      {children}
    </section>
  );
}

export function EmptyPanel({
  title,
  description,
  className,
  ...props
}: {
  title: React.ReactNode;
  description: React.ReactNode;
  className?: string;
} & Omit<HTMLAttributes<HTMLDivElement>, "title">) {
  return (
    <div
      {...props}
      className={cn(
        "rounded-[18px] bg-foreground/[0.035] px-4 py-5 shadow-[inset_0_1px_0_rgba(255,255,255,0.42)] dark:bg-white/[0.045] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.07)]",
        className,
      )}
    >
      <p className="text-sm font-medium text-foreground">{title}</p>
      <p className="mt-1 text-[13px] leading-5 text-muted-foreground">
        {description}
      </p>
    </div>
  );
}

/**
 * 居中空态：圆形图标徽章 + 标题 + 说明，整体垂直水平居中。
 * 封装收件箱 / 传输页此前各自手搓的空态结构。
 */
export function CenteredEmptyState({
  icon: Icon,
  title,
  description,
  className,
  descriptionClassName,
  ...props
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: React.ReactNode;
  description?: React.ReactNode;
  className?: string;
  descriptionClassName?: string;
} & Omit<HTMLAttributes<HTMLDivElement>, "title">) {
  return (
    <div
      {...props}
      className={cn(
        "flex h-full flex-col items-center justify-center gap-3 text-center",
        className,
      )}
    >
      <div className="flex size-14 items-center justify-center rounded-full bg-muted">
        <Icon className="size-7 text-muted-foreground" />
      </div>
      <div className="flex flex-col gap-1">
        <p className="text-sm font-medium text-foreground">{title}</p>
        {description && (
          <p
            className={cn(
              "text-xs leading-5 text-muted-foreground",
              descriptionClassName,
            )}
          >
            {description}
          </p>
        )}
      </div>
    </div>
  );
}

/**
 * 列表栏（rail）的空态：只有一行字。
 *
 * 与 [`CenteredEmptyState`] 的分工是刻意的，不是偷懒：
 * - 宽屏下列表与详情同时可见，两侧各摆一套「图标 + 标题 + 描述」就是同一句话说两遍；
 * - 窄屏用户落在**详情**侧（列表收进抽屉），所以「怎么让它变得非空」这类教学一律
 *   放详情，rail 只负责确认「这里确实是空的」。
 *
 * 收件箱与传输活动两页的 rail 共用它，别再各写一份内联 `<p>`。
 */
export function RailEmptyHint({
  children,
  className,
  ...props
}: { children: React.ReactNode } & HTMLAttributes<HTMLParagraphElement>) {
  return (
    <p
      {...props}
      className={cn(
        "px-2 py-8 text-center text-xs text-muted-foreground",
        className,
      )}
    >
      {children}
    </p>
  );
}

/**
 * 分段控件——一组互斥选项的胶囊切换条（设备页附近设备筛选、配对页模式切换）。
 *
 * 语义由 `variant` 决定：`tabs` 切换的是同一件事的两个视图（role=tab + aria-selected），
 * `filter` 只是过滤同一份列表（aria-pressed）。视觉令牌两者共用一份，改选中底色 /
 * 焦点环只需要动这里。
 */
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  variant = "filter",
  size = "sm",
  label,
  className,
  testid,
}: {
  value: T;
  options: Array<{
    value: T;
    label: React.ReactNode;
    icon?: React.ComponentType<{ className?: string }>;
    testid?: string;
  }>;
  onChange: (value: T) => void;
  variant?: "tabs" | "filter";
  size?: "sm" | "md";
  label?: string;
  className?: string;
  testid?: string;
}) {
  const isTabs = variant === "tabs";
  return (
    <div
      role={isTabs ? "tablist" : "group"}
      aria-label={label}
      data-testid={testid}
      className={cn(
        "flex shrink-0 rounded-full bg-foreground/[0.045] p-0.5 dark:bg-white/[0.06]",
        className,
      )}
    >
      {options.map((option) => {
        const active = option.value === value;
        const Icon = option.icon;
        return (
          <button
            key={option.value}
            type="button"
            role={isTabs ? "tab" : undefined}
            aria-selected={isTabs ? active : undefined}
            aria-pressed={isTabs ? undefined : active}
            data-testid={option.testid}
            onClick={() => {
              if (!active) onChange(option.value);
            }}
            className={cn(
              "focus-ring flex items-center gap-1.5 rounded-full font-medium transition-[background-color,color] duration-200",
              size === "md" ? "px-3 py-1.5 text-[12px]" : "px-2 py-1 text-[11px]",
              // 选中态两侧同为品牌色 tint。浅色此前是 `bg-zinc-950 text-white`——
              // 一块纯黑，既不在调色板里，也比同屏的主 CTA 还重（One Accent Rule）。
              active
                ? "bg-primary/15 text-brand"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {Icon ? <Icon className="size-3.5" /> : null}
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
