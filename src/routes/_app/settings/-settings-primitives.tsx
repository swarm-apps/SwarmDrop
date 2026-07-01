/**
 * Settings 共享基元
 * 统一设置页的视觉语言：分组（Section）→ 分组卡（Card）→ 设置行（Row）。
 * 所有 section 一律走这套基元，保证圆角 / 间距 / 强调色一致。
 */

import type { ComponentType, ReactNode } from "react";
import { Trans } from "@lingui/react/macro";
import { RotateCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type SettingsSectionProps = {
  title: ReactNode;
  icon?: ComponentType<{ className?: string }>;
  /** 标题右侧附加内容（如计数 Badge） */
  aside?: ReactNode;
  children: ReactNode;
  className?: string;
  /** bento 等高：撑满父高度，让同一行的卡片高度一致（搭配 grid items-stretch + SettingsCard fill） */
  fill?: boolean;
};

export function SettingsSection({
  title,
  icon: Icon,
  aside,
  children,
  className,
  fill,
}: SettingsSectionProps) {
  return (
    <section
      className={cn("flex flex-col gap-2.5", fill && "h-full", className)}
    >
      <div className="flex items-center gap-2 px-1">
        {Icon ? (
          <Icon className="size-4 shrink-0 text-blue-600 dark:text-blue-400" />
        ) : null}
        <h2 className="text-sm font-semibold tracking-tight text-foreground">
          {title}
        </h2>
        {aside ? <div className="ml-auto">{aside}</div> : null}
      </div>
      {fill ? (
        <div className="flex flex-1 flex-col gap-2.5">{children}</div>
      ) : (
        children
      )}
    </section>
  );
}

type SettingsCardProps = {
  children: ReactNode;
  className?: string;
  /** bento 等高：在 fill 的 Section 内撑满剩余高度（内容靠上，底部由玻璃底色延伸） */
  fill?: boolean;
};

/** 分组卡：一张玻璃卡，内部用 SettingsRow 的分隔线划分，不做卡片套卡片 */
export function SettingsCard({ children, className, fill }: SettingsCardProps) {
  return (
    <div
      className={cn(
        "glass-card overflow-hidden rounded-[20px]",
        fill && "flex flex-1 flex-col",
        className,
      )}
    >
      {children}
    </div>
  );
}

type SettingsRowProps = {
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  className?: string;
  contentClassName?: string;
};

/** 设置行：左侧标题+描述，右侧操作控件；同卡内靠下边框分隔 */
export function SettingsRow({
  title,
  description,
  action,
  className,
  contentClassName,
}: SettingsRowProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 border-b border-border/60 p-4 last:border-b-0",
        className,
      )}
    >
      <div className={cn("flex min-w-0 flex-col gap-0.5", contentClassName)}>
        <div className="text-sm font-medium text-foreground">{title}</div>
        {description ? (
          <span className="text-xs leading-5 text-muted-foreground">
            {description}
          </span>
        ) : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}

type NodeRestartBannerProps = {
  message: ReactNode;
  restarting: boolean;
  onRestart: () => void;
};

/** 节点设置变更后的「需重启」提示条，网络 / 引导节点共用 */
export function NodeRestartBanner({
  message,
  restarting,
  onRestart,
}: NodeRestartBannerProps) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-2xl border border-amber-300/70 bg-amber-50 px-3.5 py-2.5 dark:border-amber-900/60 dark:bg-amber-950/40">
      <span className="text-xs leading-5 text-amber-800 dark:text-amber-200">
        {message}
      </span>
      <Button
        size="sm"
        variant="outline"
        className="h-7 shrink-0 text-xs"
        onClick={onRestart}
        disabled={restarting}
      >
        <RotateCw className={cn("mr-1 size-3", restarting && "animate-spin")} />
        {restarting ? <Trans>重启中...</Trans> : <Trans>重启节点</Trans>}
      </Button>
    </div>
  );
}
