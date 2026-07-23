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
      className={cn(
        "glass-panel flex min-h-full flex-col gap-4 rounded-[24px] p-4",
        className,
      )}
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
              active
                ? "bg-zinc-950 text-white dark:bg-primary/20 dark:text-brand"
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
