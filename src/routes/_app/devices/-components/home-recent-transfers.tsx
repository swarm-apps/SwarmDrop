/**
 * HomeRecentTransfers
 * 主屏底部"最近传输"区 —— 显示最多 5 条历史摘要,点击行进入详情,"查看全部"跳 /transfer
 */

import { Link } from "@tanstack/react-router";
import {
  ChevronRight,
  CheckCircle2,
  XCircle,
  ArrowRight,
  ArrowLeft,
} from "lucide-react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useTransferStore } from "@/stores/transfer-store";
import { formatRelativeTime } from "@/lib/format";
import type { TransferHistoryItem } from "@/lib/bindings";
import { cn } from "@/lib/utils";

const MAX_ROWS = 5;

export function HomeRecentTransfers() {
  const dbHistory = useTransferStore((s) => s.dbHistory);
  const recent = dbHistory.slice(0, MAX_ROWS);

  if (recent.length === 0) return null;

  return (
    <section className="flex flex-col gap-1">
      <div className="flex items-center justify-between pb-1.5">
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>最近传输</Trans>
        </h2>
        <Link
          to="/transfer"
          className="flex items-center gap-1 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
        >
          <Trans>查看全部</Trans>
          <ChevronRight className="size-3.5" />
        </Link>
      </div>

      <ul className="flex flex-col">
        {recent.map((item, idx) => (
          <RecentRow
            key={item.sessionId}
            item={item}
            isLast={idx === recent.length - 1}
          />
        ))}
      </ul>
    </section>
  );
}

function RecentRow({
  item,
  isLast,
}: {
  item: TransferHistoryItem;
  isLast: boolean;
}) {
  const { t } = useLingui();
  const isSend = item.direction === "send";
  const isCompleted = item.status === "completed";
  const fileCount = item.files.length;
  const firstName = item.files[0]?.name ?? t`未知文件`;
  const displayName =
    fileCount > 1 ? t`${firstName} 等 ${fileCount} 个文件` : firstName;

  return (
    <Link
      to="/transfer/$sessionId"
      params={{ sessionId: item.sessionId }}
      className={cn(
        "flex items-center gap-3 py-2.5 transition-colors hover:bg-accent/40",
        !isLast && "border-b border-border",
      )}
    >
      {/* 状态图标 */}
      <span
        className={cn(
          "flex size-7 shrink-0 items-center justify-center rounded-full",
          isCompleted ? "bg-secondary" : "bg-red-50 dark:bg-red-950/40",
        )}
      >
        {isCompleted ? (
          <CheckCircle2 className="size-4 text-green-600" />
        ) : (
          <XCircle className="size-4 text-destructive" />
        )}
      </span>

      {/* 文件名 */}
      <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
        {displayName}
      </span>

      {/* 方向 + 设备 */}
      <span className="flex shrink-0 items-center gap-1.5 text-[13px] text-muted-foreground">
        {isSend ? (
          <ArrowRight className="size-3.5" />
        ) : (
          <ArrowLeft className="size-3.5" />
        )}
        <span className="max-w-[8em] truncate">{item.peerName}</span>
      </span>

      {/* 时间 */}
      <span className="shrink-0 text-xs text-muted-foreground">
        {formatRelativeTime(item.finishedAt ?? item.startedAt)}
      </span>
    </Link>
  );
}
