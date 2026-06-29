/**
 * TransferItem
 * 传输记录卡片 — 显示传输进度/状态
 * 通过 sessionId 独立订阅 store，避免父组件重渲染
 */

import { memo, useCallback, useState } from "react";
import {
  X,
  Pause,
  CheckCircle2,
  XCircle,
  Loader2,
  FolderOpen,
} from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { useTransferStore } from "@/stores/transfer-store";
import {
  formatFileSize,
  formatSpeed,
  formatDuration,
  formatRelativeTime,
} from "@/lib/format";
import { Progress } from "@/components/ui/progress";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { getErrorMessage } from "@/lib/errors";
import { openTransferResult } from "@/lib/file-picker";
import { useNavigate } from "@tanstack/react-router";
import type { TransferProjection } from "@/lib/bindings";
import { projectionStatusLabel, projectionToSession } from "@/lib/transfer-projection";
import {
  DirectionIcon,
  TransferCard,
  calcPercent,
  isActiveStatus,
  doPauseTransfer,
  doCancelTransfer,
  ACTION_BTN_CLASS,
  DESTRUCTIVE_BTN_CLASS,
} from "./-shared";

interface TransferItemProps {
  projection: TransferProjection;
}

export const TransferItem = memo(function TransferItem({
  projection,
}: TransferItemProps) {
  const progress = useTransferStore(
    useCallback(
      (s) => s.progressBySession[projection.sessionId] ?? null,
      [projection.sessionId],
    ),
  );
  const session = projectionToSession(projection, progress);
  const sessionId = projection.sessionId;
  const navigate = useNavigate();
  const [isCancelling, setIsCancelling] = useState(false);

  const handleClick = useCallback(() => {
    navigate({
      to: "/transfer/$sessionId",
      params: { sessionId },
    });
  }, [navigate, sessionId]);

  const handlePause = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      try {
        await doPauseTransfer(sessionId);
      } catch {
        // doPauseTransfer 已 toast
      }
    },
    [sessionId],
  );

  const handleCancel = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      if (isCancelling) return;
      setIsCancelling(true);
      try {
        await doCancelTransfer(sessionId, session.direction);
      } catch {
        setIsCancelling(false);
      }
    },
    [isCancelling, sessionId, session.direction],
  );

  const handleOpenFolder = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      try {
        await openTransferResult(session);
      } catch (err) {
        toast.error(getErrorMessage(err));
      }
    },
    [session],
  );

  const isSend = session.direction === "send";
  const isActive = isActiveStatus(session.status);
  const progressPercent = session.progress
    ? calcPercent(session.progress.transferredBytes, session.progress.totalBytes)
    : 0;
  const activeFileName = session.progress?.files?.find(
    (f) => f.status === "transferring",
  )?.name;

  return (
    <TransferCard onClick={handleClick} alignItems="start">
      <DirectionIcon isSend={isSend} />

      {/* 中间：详细信息 */}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5 md:gap-1">
        <h3 className="truncate text-[13px] font-medium text-foreground md:text-sm">
          {isSend ? (
            <Trans>发送到 {session.deviceName}</Trans>
          ) : (
            <Trans>来自 {session.deviceName}</Trans>
          )}
        </h3>

        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground md:text-xs">
          <span>
            {session.files.length} <Trans>个文件</Trans>
          </span>
          <span className="text-muted-foreground/40">·</span>
          <span>{formatFileSize(session.totalSize)}</span>
        </div>

        {/* 状态区域 */}
        <div className="mt-0.5">
          {session.status === "transferring" && session.progress && (
            <div className="flex flex-col gap-1.5 mt-0.5">
              <Progress value={progressPercent} className="h-1.5" />
              <div className="flex items-center justify-between text-[11px] md:text-[12px]">
                <span className="flex items-center gap-1 text-blue-600 dark:text-blue-400">
                  <Loader2 className="size-3 animate-spin md:size-3.5" />
                  <span className="max-w-[8em] truncate md:max-w-[12em]">
                    {activeFileName || t`传输中`}
                  </span>
                </span>
                <span className="text-muted-foreground">
                  {formatSpeed(session.progress.speed)} · {progressPercent}%
                  {session.progress.eta != null && (
                    <span className="hidden md:inline">
                      {" "}· <Trans>剩余 {formatDuration(session.progress.eta)}</Trans>
                    </span>
                  )}
                </span>
              </div>
            </div>
          )}

          {session.status === "waiting_accept" && (
            <div className="flex items-center gap-1.5 text-[12px] text-amber-600 dark:text-amber-400 md:text-[13px]">
              <Loader2 className="size-3 animate-spin md:size-3.5" />
              <Trans>等待对方确认...</Trans>
            </div>
          )}

          {session.status === "pending" && (
            <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground md:text-[13px]">
              <Loader2 className="size-3 animate-spin md:size-3.5" />
              <Trans>准备中...</Trans>
            </div>
          )}

          {session.status === "completed" && (
            <div className="flex items-center gap-1.5 text-[12px] text-green-600 dark:text-green-400 md:text-[13px]">
              <CheckCircle2 className="size-3.5 md:size-4" />
              <Trans>传输完成</Trans>
              {session.completedAt && (
                <span className="text-muted-foreground">
                  / {formatRelativeTime(session.completedAt)}
                </span>
              )}
            </div>
          )}

          {session.status === "failed" && (
            <div className="flex items-center gap-1.5 text-[12px] text-destructive md:text-[13px]">
              <XCircle className="size-3.5 shrink-0 md:size-4" />
              <span className="truncate">
                {session.error || projectionStatusLabel(projection)}
              </span>
            </div>
          )}

          {session.status === "cancelled" && (
            <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground md:text-[13px]">
              <XCircle className="size-3.5 md:size-4" />
              {projectionStatusLabel(projection)}
              {session.completedAt && (
                <span className="hidden md:inline">
                  / {formatRelativeTime(session.completedAt)}
                </span>
              )}
            </div>
          )}
        </div>
      </div>

      {/* 右侧：操作按钮 */}
      <div className="flex shrink-0 items-start gap-0.5 -mr-1 md:-mr-1.5">
        {session.status === "transferring" && (
          <Button size="icon" variant="ghost" className={ACTION_BTN_CLASS} onClick={handlePause} title={t`暂停传输`}>
            <Pause className="size-3.5 md:size-4" />
          </Button>
        )}
        {isActive && (
          <Button
            size="icon"
            variant="ghost"
            className={DESTRUCTIVE_BTN_CLASS}
            onClick={handleCancel}
            disabled={isCancelling}
            title={isCancelling ? t`取消中...` : t`取消传输`}
          >
            {isCancelling ? (
              <Loader2 className="size-3.5 animate-spin md:size-4" />
            ) : (
              <X className="size-3.5 md:size-4" />
            )}
          </Button>
        )}
        {session.status === "completed" && session.saveLocation && (
          <Button size="icon" variant="ghost" className={ACTION_BTN_CLASS} onClick={handleOpenFolder} title={t`打开文件夹`}>
            <FolderOpen className="size-3.5 md:size-4" />
          </Button>
        )}
      </div>
    </TransferCard>
  );
});
