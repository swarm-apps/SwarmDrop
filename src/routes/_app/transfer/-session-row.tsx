/**
 * SessionRow
 * 活动中心左栏的统一会话行 —— 活跃 / 可恢复 / 终态共用一种行结构，
 * 按 phase 渲染状态区与行内操作。通过 sessionId 独立订阅进度，避免父列表高频重渲染。
 */

import { memo, useState } from "react";
import {
  CheckCircle2,
  Loader2,
  Pause,
  Play,
  Trash2,
  X,
  XCircle,
} from "lucide-react";
import { Trans, useLingui } from "@lingui/react/macro";
import { toast } from "sonner";
import type { TransferProjection } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { useSessionProgress, useTransferStore } from "@/stores/transfer-store";
import {
  calcPercent,
  formatFileSize,
  formatSpeed,
  formatRelativeTime,
} from "@/lib/format";
import { getErrorMessage } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { PolicyReasonBadge } from "@/components/transfer/policy-reason-badge";
import { DirectionIcon } from "@/components/transfer/session-panel";
import {
  doCancelTransfer,
  doPauseTransfer,
  doResumeTransfer,
} from "@/lib/transfer-actions";
import {
  canResumeProjection,
  isProjectionActive,
  isProjectionCancelled,
  isProjectionCompleted,
  isProjectionFailed,
  projectionStatusLabel,
} from "@/lib/transfer-projection";

export const ACTION_BTN_CLASS =
  "size-7 text-muted-foreground hover:bg-accent hover:text-foreground";
export const DESTRUCTIVE_BTN_CLASS =
  "size-7 text-muted-foreground hover:bg-destructive/10 hover:text-destructive";

interface SessionRowProps {
  projection: TransferProjection;
  selected: boolean;
  /** 点击选中该会话（接收 sessionId，便于父级直接透传稳定回调、保住 memo） */
  onSelect: (sessionId: string) => void;
  /** 恢复传输产生新会话时回调（选中新会话） */
  onSessionChange: (newSessionId: string) => void;
}

export const SessionRow = memo(function SessionRow({
  projection,
  selected,
  onSelect,
  onSessionChange,
}: SessionRowProps) {
  const { t } = useLingui();
  const sessionId = projection.sessionId;
  const progress = useSessionProgress(sessionId);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  const [isCancelling, setIsCancelling] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const withAction =
    (action: () => Promise<void>) => async (e: React.MouseEvent) => {
      e.stopPropagation();
      try {
        await action();
      } catch (err) {
        toast.error(getErrorMessage(err));
      }
    };

  const onPause = withAction(() => doPauseTransfer(sessionId));
  const onResume = withAction(async () => {
    const newSessionId = await doResumeTransfer(sessionId);
    onSessionChange(newSessionId);
  });
  const handleDeleteConfirm = async () => {
    setDeleteOpen(false);
    try {
      await commands.deleteTransferSession(sessionId);
      await loadProjections();
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };
  const onCancel = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isCancelling) return;
    setIsCancelling(true);
    try {
      await doCancelTransfer(sessionId, projection.direction);
    } catch {
      // doCancelTransfer 已 toast
    } finally {
      setIsCancelling(false);
    }
  };

  const isSend = projection.direction === "send";
  const isActive = isProjectionActive(projection);
  const isSuspended = projection.phase === "suspended";
  const canResume = canResumeProjection(projection);

  const fileCount = projection.files.length;
  const firstFileName = projection.files[0]?.name || t`未知文件`;
  const displayFileName =
    fileCount > 1 ? t`${firstFileName} 等 ${fileCount} 个文件` : firstFileName;

  const progressPercent = progress
    ? calcPercent(progress.transferredBytes, progress.totalBytes)
    : calcPercent(projection.transferredBytes ?? 0, projection.totalSize);
  const activeFileName = progress?.files?.find(
    (f) => f.status === "transferring",
  )?.name;

  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      onClick={() => onSelect(sessionId)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(sessionId);
        }
      }}
      className={cn(
        "group flex cursor-pointer items-start gap-2.5 rounded-[18px] p-3 text-left transition-[background-color,border-color,box-shadow,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.995]",
        selected
          ? "glass-accent shadow-[0_10px_24px_rgba(219,163,65,0.12)]"
          : "glass-card hover:border-primary/25",
      )}
    >
      <DirectionIcon isSend={isSend} />

      {/* 中间：核心信息 */}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <h3
          className="truncate text-[13px] font-medium text-foreground"
          title={displayFileName}
        >
          {displayFileName}
        </h3>

        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <span className="shrink-0">
            {isSend ? <Trans>发送到</Trans> : <Trans>来自</Trans>}
          </span>
          <span className="max-w-[10em] truncate font-medium text-foreground/80">
            {projection.peerName}
          </span>
          <span className="shrink-0 text-muted-foreground/40">·</span>
          <span className="shrink-0 font-mono tabular-nums">
            {formatFileSize(projection.totalSize)}
          </span>
        </div>

        {/* 状态区 */}
        <div className="mt-0.5">
          {projection.phase === "active" && progress && (
            <div className="mt-0.5 flex flex-col gap-1.5">
              <Progress value={progressPercent} className="h-1.5" />
              <div className="flex items-center justify-between text-[11px]">
                <span className="flex min-w-0 items-center gap-1 text-brand">
                  <Loader2 className="size-3 shrink-0 animate-spin" />
                  <span className="truncate">
                    {activeFileName || t`传输中`}
                  </span>
                </span>
                <span className="shrink-0 font-mono tabular-nums text-muted-foreground">
                  {formatSpeed(progress.speed)} · {progressPercent}%
                </span>
              </div>
            </div>
          )}

          {(projection.phase === "waiting_accept" ||
            projection.phase === "offered") && (
            <div
              className={cn(
                "flex items-center gap-1.5 text-[12px]",
                projection.phase === "waiting_accept"
                  ? "text-amber-600 dark:text-amber-400"
                  : "text-muted-foreground",
              )}
            >
              <Loader2 className="size-3 animate-spin" />
              {projectionStatusLabel(projection)}
            </div>
          )}

          {isSuspended && (
            <div className="mt-0.5 flex flex-col gap-1.5">
              <Progress value={progressPercent} className="h-1.5" />
              <div className="flex items-center justify-between text-[11px]">
                <span className="flex items-center gap-1 text-amber-600 dark:text-amber-400">
                  <Pause className="size-3" />
                  {projectionStatusLabel(projection)}
                </span>
                <span className="font-mono tabular-nums text-muted-foreground">
                  {progressPercent}%
                </span>
              </div>
            </div>
          )}

          {isProjectionCompleted(projection) && (
            <div className="flex items-center gap-1.5 text-[12px] text-green-600 dark:text-green-400">
              <CheckCircle2 className="size-3.5" />
              {projectionStatusLabel(projection)}
            </div>
          )}

          {isProjectionFailed(projection) && (
            <div className="flex items-center gap-1.5 text-[12px] text-destructive">
              <XCircle className="size-3.5 shrink-0" />
              <span className="truncate">
                {projection.errorMessage || projectionStatusLabel(projection)}
              </span>
            </div>
          )}

          {isProjectionCancelled(projection) && (
            <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
              <XCircle className="size-3.5" />
              {projectionStatusLabel(projection)}
            </div>
          )}

          <PolicyReasonBadge
            policyAction={projection.policyAction}
            policyReason={projection.policyReason}
          />
        </div>
      </div>

      {/* 右列：时间 + 行内操作 */}
      <div className="-mr-1 flex shrink-0 flex-col items-end gap-1">
        <span className="text-[10px] text-muted-foreground">
          {formatRelativeTime(projection.finishedAt || projection.startedAt)}
        </span>
        <div className="flex items-center gap-0.5">
          {projection.phase === "active" && (
            <Button
              size="icon"
              variant="ghost"
              className={ACTION_BTN_CLASS}
              onClick={onPause}
              title={t`暂停传输`}
            >
              <Pause className="size-3.5" />
            </Button>
          )}
          {isActive && (
            <Button
              size="icon"
              variant="ghost"
              className={DESTRUCTIVE_BTN_CLASS}
              onClick={onCancel}
              disabled={isCancelling}
              title={isCancelling ? t`取消中...` : t`取消传输`}
            >
              {isCancelling ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <X className="size-3.5" />
              )}
            </Button>
          )}
          {canResume && (
            <Button
              size="icon"
              variant="ghost"
              className={ACTION_BTN_CLASS}
              onClick={onResume}
              title={t`恢复传输`}
            >
              <Play className="size-3.5" />
            </Button>
          )}
          {!isActive && (
            <Button
              size="icon"
              variant="ghost"
              className={DESTRUCTIVE_BTN_CLASS}
              onClick={(e) => {
                e.stopPropagation();
                setDeleteOpen(true);
              }}
              title={t`删除记录`}
            >
              <Trash2 className="size-3.5" />
            </Button>
          )}
        </div>
      </div>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent onClick={(e) => e.stopPropagation()}>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>删除「{displayFileName}」的传输记录？</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              {canResume ? (
                <Trans>
                  删除后该任务的断点信息将一并清除，无法再继续续传；已传输的文件不受影响。
                </Trans>
              ) : (
                <Trans>记录删除后无法恢复；已传输的文件不受影响。</Trans>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              <Trans>取消</Trans>
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteConfirm}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              <Trans>删除记录</Trans>
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
});
