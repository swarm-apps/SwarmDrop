/**
 * FileTreeItem
 * 文件树中的文件行组件，支持 5 种状态变体
 */

import { Check, Timer, X, RotateCcw } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { cn } from "@/lib/utils";
import { formatFileSize } from "@/lib/format";
import { getFileIcon } from "@/lib/file-icon";
import { Progress } from "@/components/ui/progress";
import type { FileStatus } from "./data";

interface FileTreeItemProps {
  name: string;
  size: number;
  variant: FileStatus;
  /** 传输进度 0-100（仅 transferring 用） */
  progress?: number;
  /** 缩进层级 */
  level?: number;
  /** 删除回调（select 模式） */
  onRemove?: () => void;
  /** 重试回调（error 模式） */
  onRetry?: () => void;
}

/* ─── 共享删除按钮 ─── */

export function RemoveButton({ onClick }: { onClick?: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="cursor-pointer rounded-sm p-0.5 text-muted-foreground/70 transition-colors hover:bg-destructive/10 hover:text-destructive"
    >
      <X className="size-3.5" />
    </button>
  );
}

/* ─── 变体样式 ─── */

const variantStyles: Record<
  FileStatus,
  { row: string; icon: string; name: string; info: string }
> = {
  select: {
    row: "hover:bg-muted/50",
    icon: "text-brand",
    name: "text-foreground",
    info: "text-muted-foreground",
  },
  waiting: {
    row: "opacity-55",
    icon: "text-muted-foreground",
    name: "text-muted-foreground",
    info: "text-muted-foreground",
  },
  transferring: {
    row: "bg-primary/8 border border-primary/25 dark:bg-primary/10 dark:border-primary/25",
    icon: "text-brand",
    name: "text-foreground",
    info: "text-brand font-medium",
  },
  completed: {
    row: "bg-green-50/40 dark:bg-green-500/5",
    icon: "text-green-500",
    name: "text-foreground",
    info: "text-muted-foreground",
  },
  error: {
    row: "bg-red-50 dark:bg-red-950/20",
    icon: "text-red-500",
    name: "text-foreground",
    info: "text-red-500",
  },
};

/* ─── 状态尾标 ─── */

const STATUS_TRAIL: Partial<
  Record<FileStatus, React.ComponentType<{ className?: string }>>
> = {
  waiting: Timer,
  completed: Check,
};

function renderTrailIcon(variant: FileStatus, infoClass: string) {
  const TrailIcon = STATUS_TRAIL[variant];
  if (!TrailIcon) return null;
  return (
    <TrailIcon
      className={cn(
        "size-3.5",
        variant === "completed" ? "text-green-500" : infoClass,
      )}
    />
  );
}

/* ─── 主组件 ─── */

export function FileTreeItem({
  name,
  size,
  variant,
  progress = 0,
  level = 0,
  onRemove,
  onRetry,
}: FileTreeItemProps) {
  const styles = variantStyles[variant];
  const Icon = getFileIcon(name);
  const isTransferring = variant === "transferring";

  // 图标 + 文件名行（所有变体共享）
  const nameRow = (
    <div className="flex items-center gap-2.5">
      <div className="flex min-w-0 flex-1 items-center gap-2.5">
        <Icon className={cn("size-4.5 shrink-0", styles.icon)} />
        <span className={cn("min-w-0 truncate text-sm", styles.name)}>
          {name}
        </span>
      </div>

      {/* 右侧信息区 */}
      <div className="flex shrink-0 items-center gap-1.5">
        <span className={cn("text-xs", styles.info)}>
          {isTransferring
            ? `${Math.round(progress)}%`
            : variant === "error"
              ? <Trans>失败</Trans>
              : formatFileSize(size)}
        </span>
        {variant === "select" && onRemove && <RemoveButton onClick={onRemove} />}
        {variant === "error" && (
          <button
            type="button"
            onClick={onRetry}
            className="rounded p-0.5 text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30"
          >
            <RotateCcw className="size-3.5" />
          </button>
        )}
        {variant in STATUS_TRAIL &&
          renderTrailIcon(variant, styles.info)}
      </div>
    </div>
  );

  return (
    <div
      className={cn(
        "flex flex-col rounded-lg px-2.5 py-2",
        isTransferring ? "gap-1.5" : "gap-0",
        styles.row,
      )}
      style={{ paddingLeft: `${level * 22 + 8}px` }}
    >
      {nameRow}
      {isTransferring && <Progress value={progress} className="h-1.5" />}
    </div>
  );
}
