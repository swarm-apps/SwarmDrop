/**
 * Transfer 共享组件和工具函数
 * 在 TransferItem、HistoryItem、详情页之间复用
 */

import { ArrowUpRight, ArrowDownLeft } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import { getErrorMessage } from "@/lib/errors";
import { commands, type TransferProjection } from "@/lib/bindings";
import { projectionStatusLabel } from "@/lib/transfer-projection";

export { projectionStatusLabel };

/* ─── 方向图标 ─── */

export function DirectionIcon({ isSend }: { isSend: boolean }) {
  return (
    <div
      className={cn(
        "flex size-9 shrink-0 items-center justify-center rounded-lg md:size-10 md:rounded-xl",
        isSend
          ? "bg-primary/10 text-brand dark:bg-primary/15"
          : "bg-green-50 text-green-500 dark:bg-green-500/15 dark:text-green-400",
      )}
    >
      {isSend ? (
        <ArrowUpRight className="size-4 md:size-4.5" strokeWidth={2.5} />
      ) : (
        <ArrowDownLeft className="size-4 md:size-4.5" strokeWidth={2.5} />
      )}
    </div>
  );
}

/* ─── 卡片容器 ─── */

const CARD_BASE =
  "glass-card group relative flex cursor-pointer items-center gap-2.5 rounded-xl p-3 transition-[border-color,box-shadow,transform] hover:border-primary/25 hover:shadow-sm active:scale-[0.995] md:gap-3 md:p-3.5";

export function TransferCard({
  onClick,
  alignItems = "center",
  children,
}: {
  onClick: () => void;
  alignItems?: "start" | "center";
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(CARD_BASE, alignItems === "start" && "items-start md:items-start")}
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") onClick();
      }}
    >
      {children}
    </div>
  );
}

/* ─── 工具函数 ─── */

/** 计算传输进度百分比 */
export function calcPercent(transferred: number, total: number): number {
  return total > 0 ? Math.round((transferred / total) * 100) : 0;
}

/** 操作按钮通用样式 */
export const ACTION_BTN_CLASS =
  "size-7 text-muted-foreground hover:bg-accent hover:text-foreground md:size-8";
export const DESTRUCTIVE_BTN_CLASS =
  "size-7 text-muted-foreground hover:bg-destructive/10 hover:text-destructive md:size-8";

/* ─── 状态徽章样式 ─── */

const STATUS_CLASSNAMES = {
  pending: "bg-gray-100 text-gray-600 dark:bg-gray-500/15 dark:text-gray-400",
  waiting_accept:
    "bg-yellow-100 text-yellow-600 dark:bg-yellow-500/15 dark:text-yellow-400",
  transferring:
    "bg-primary/15 text-brand dark:bg-primary/15",
  paused:
    "bg-amber-100 text-amber-700 dark:bg-amber-500/15 dark:text-amber-300",
  completed:
    "bg-green-100 text-green-600 dark:bg-green-500/15 dark:text-green-400",
  failed: "bg-red-100 text-red-600 dark:bg-red-500/15 dark:text-red-400",
  cancelled:
    "bg-gray-100 text-gray-600 dark:bg-gray-500/15 dark:text-gray-400",
} as const;

/** 由 projection 派生状态徽章的配色类名（与 projectionStatusLabel 同一套 phase 语义）。 */
export function projectionStatusClassName(
  projection: TransferProjection,
): string {
  switch (projection.phase) {
    case "offered":
      return STATUS_CLASSNAMES.pending;
    case "waiting_accept":
      return STATUS_CLASSNAMES.waiting_accept;
    case "active":
      return STATUS_CLASSNAMES.transferring;
    case "suspended":
      return STATUS_CLASSNAMES.paused;
    case "terminal":
      switch (projection.terminalReason) {
        case "completed":
          return STATUS_CLASSNAMES.completed;
        case "cancelled":
        case "rejected":
          return STATUS_CLASSNAMES.cancelled;
        default:
          return STATUS_CLASSNAMES.failed;
      }
  }
}

/* ─── 传输操作（核心逻辑） ─── */

// 这些操作的状态变化由后端 projection-update 事件回流（applyProjection），
// 不再各自 loadProjections——避免冗余全量往返与乱序覆盖。

/** 暂停传输 */
export async function doPauseTransfer(sessionId: string) {
  try {
    await commands.pauseTransfer(sessionId);
  } catch (err) {
    toast.error(getErrorMessage(err));
    throw err;
  }
}

/** 取消传输 */
export async function doCancelTransfer(
  sessionId: string,
  direction: "send" | "receive",
) {
  try {
    if (direction === "send") {
      await commands.cancelSend(sessionId);
    } else {
      await commands.cancelReceive(sessionId);
    }
    toast.success(t`已取消传输`);
  } catch (err) {
    toast.error(getErrorMessage(err));
    throw err;
  }
}

/** 恢复传输 */
export async function doResumeTransfer(sessionId: string): Promise<string> {
  const result = await commands.resumeTransfer(sessionId);
  if (result.direction !== "send" && result.direction !== "receive") {
    throw new Error(
      `resume_transfer returned invalid direction "${result.direction}" for ${sessionId}`,
    );
  }
  return result.sessionId;
}
