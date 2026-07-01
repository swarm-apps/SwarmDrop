/**
 * 通用区块布局原语
 * 玻璃面板外壳、区块标题、内嵌空态面板、居中空态——跨页面（设备 / 收件箱 / 传输）复用。
 */

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
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section
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
}: {
  title: React.ReactNode;
  description: React.ReactNode;
  className?: string;
}) {
  return (
    <div
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
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: React.ReactNode;
  description?: React.ReactNode;
  className?: string;
  descriptionClassName?: string;
}) {
  return (
    <div
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
