/**
 * 会话详情面板组件族
 * 活动中心（/transfer 右栏 / 抽屉）与发送流（/send、/send/share-target 就地进度）共用：
 * - SessionSummaryHeader：对端设备 + 方向摘要 + 状态徽章
 * - SessionProgressBlock：按 phase 渲染进度 / 完成摘要 / 失败信息
 * - SessionFileSection：文件明细树
 * - SessionActions：暂停 / 恢复 / 取消 / 打开文件夹（不再隐式跳转，由使用方回调决定去向）
 */

import { memo, useCallback, useMemo, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import {
  ArrowDownLeft,
  ArrowUpRight,
  CheckCircle2,
  FolderOpen,
  Loader2,
  Pause,
  Play,
  RotateCcw,
  X,
  XCircle,
} from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import {
  calcPercent,
  formatFileSize,
  formatSpeed,
  formatDuration,
} from "@/lib/format";
import { cn } from "@/lib/utils";
import { openTransferResult } from "@/lib/file-picker";
import { getErrorMessage } from "@/lib/errors";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { FileTree, buildTreeDataFromSession } from "@/components/file-tree";
import type {
  TransferProjection,
  TransferProgressEvent,
} from "@/lib/bindings";
import {
  doPauseTransfer,
  doCancelTransfer,
  doResendTransfer,
  doResumeTransfer,
} from "@/lib/transfer-actions";
import {
  canResendProjection,
  canResumeProjection,
  isProjectionActive,
  isProjectionCompleted,
  isProjectionFailed,
  projectionStatusLabel,
} from "@/lib/transfer-projection";

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

/* ─── 状态徽章配色（与 projectionStatusLabel 同一套 phase 语义） ─── */

const STATUS_CLASSNAMES = {
  pending: "bg-gray-100 text-gray-600 dark:bg-gray-500/15 dark:text-gray-400",
  waiting_accept:
    "bg-yellow-100 text-yellow-600 dark:bg-yellow-500/15 dark:text-yellow-400",
  transferring: "bg-primary/15 text-brand dark:bg-primary/15",
  paused:
    "bg-amber-100 text-amber-700 dark:bg-amber-500/15 dark:text-amber-300",
  completed:
    "bg-green-100 text-green-600 dark:bg-green-500/15 dark:text-green-400",
  failed: "bg-red-100 text-red-600 dark:bg-red-500/15 dark:text-red-400",
  cancelled:
    "bg-gray-100 text-gray-600 dark:bg-gray-500/15 dark:text-gray-400",
} as const;

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

/* ─── 会话摘要头 ─── */

export const SessionSummaryHeader = memo(function SessionSummaryHeader({
  projection,
}: {
  projection: TransferProjection;
}) {
  const isSend = projection.direction === "send";

  // 从 network store / secret store 查找设备 OS
  const deviceOs = useNetworkStore(
    (s) => s.devices.find((d) => d.peerId === projection.peerId)?.os,
  );
  const pairedOs = useSecretStore(
    (s) => s.pairedDevices.find((d) => d.peerId === projection.peerId)?.os,
  );
  const os = deviceOs ?? pairedOs ?? "";
  const DeviceIcon = getDeviceIcon(os);

  return (
    <div className="flex items-center gap-3 md:gap-3.5">
      <div
        className={cn(
          "flex size-11 shrink-0 items-center justify-center rounded-full md:size-12",
          isSend
            ? "bg-primary/10 dark:bg-primary/15"
            : "bg-green-50 dark:bg-green-500/15",
        )}
      >
        <DeviceIcon
          className={cn(
            "size-5.5 md:size-6",
            isSend ? "text-brand" : "text-green-600 dark:text-green-400",
          )}
        />
      </div>
      <div className="min-w-0 flex-1">
        <h2 className="truncate text-[15px] font-semibold text-foreground md:text-base">
          {projection.peerName}
        </h2>
        <p className="text-[11px] text-muted-foreground md:text-xs">
          {isSend ? <Trans>发送</Trans> : <Trans>接收</Trans>}
          {" · "}
          {projection.files.length} <Trans>个文件</Trans>
          {" · "}
          {formatFileSize(projection.totalSize)}
        </p>
      </div>
      <span
        className={cn(
          "shrink-0 rounded-full px-2 py-0.5 text-xs font-medium md:px-2.5",
          projectionStatusClassName(projection),
        )}
      >
        {projectionStatusLabel(projection)}
      </span>
    </div>
  );
});

/* ─── 进度块 ─── */

export const SessionProgressBlock = memo(function SessionProgressBlock({
  projection,
  progress,
}: {
  projection: TransferProjection;
  progress: TransferProgressEvent | null;
}) {
  const progressPercent = progress
    ? calcPercent(progress.transferredBytes, progress.totalBytes)
    : 0;

  // 可恢复 / 中断的 suspended 会话
  if (projection.phase === "suspended") {
    const pausedPercent = calcPercent(
      projection.transferredBytes ?? 0,
      projection.totalSize,
    );
    return (
      <div className="flex flex-col gap-2 md:gap-2.5">
        <div className="flex items-baseline justify-between">
          <span className="font-mono text-2xl font-bold tabular-nums text-foreground md:text-3xl">
            {pausedPercent}%
          </span>
          <span className="text-[11px] text-muted-foreground md:text-xs">
            {projectionStatusLabel(projection)}
          </span>
        </div>
        <Progress value={pausedPercent} className="h-1.5 md:h-2" />
        <div className="flex items-center justify-between font-mono text-[11px] tabular-nums text-muted-foreground md:text-xs">
          <span>
            {formatFileSize(projection.transferredBytes ?? 0)} /{" "}
            {formatFileSize(projection.totalSize)}
          </span>
        </div>
      </div>
    );
  }

  if (projection.phase === "active" && progress) {
    return (
      <div className="flex flex-col gap-2 md:gap-2.5">
        <div className="flex items-baseline justify-between">
          <span className="font-mono text-2xl font-bold tabular-nums text-foreground md:text-3xl">
            {progressPercent}%
          </span>
          <span className="font-mono text-[11px] tabular-nums text-muted-foreground md:text-xs">
            {formatSpeed(progress.speed)}
          </span>
        </div>
        <Progress value={progressPercent} className="h-1.5 md:h-2" />
        <div className="flex items-center justify-between font-mono text-[11px] tabular-nums text-muted-foreground md:text-xs">
          <span>
            {formatFileSize(progress.transferredBytes)} /{" "}
            {formatFileSize(progress.totalBytes)}
          </span>
          {progress.eta != null && (
            <span>
              <Trans>剩余 {formatDuration(progress.eta)}</Trans>
            </span>
          )}
        </div>
      </div>
    );
  }

  if (isProjectionCompleted(projection)) {
    const duration = projection.finishedAt
      ? Math.round((projection.finishedAt - projection.startedAt) / 1000)
      : 0;

    return (
      <div className="flex flex-col items-center gap-2.5 py-2 md:gap-3 md:py-4">
        <div className="flex size-14 items-center justify-center rounded-full bg-green-100 dark:bg-green-500/15 md:size-16">
          <CheckCircle2 className="size-7 text-green-600 dark:text-green-400 md:size-8" />
        </div>
        <h3 className="text-base font-semibold text-foreground md:text-lg">
          <Trans>所有文件传输完成！</Trans>
        </h3>

        <div className="flex w-full max-w-xs justify-between px-4 md:max-w-sm md:justify-center md:gap-8 md:px-0">
          <div className="flex flex-col items-center gap-0.5">
            <span className="font-mono text-lg font-bold tabular-nums text-foreground md:text-xl">
              {projection.files.length}
            </span>
            <span className="text-[10px] text-muted-foreground md:text-[11px]">
              <Trans>文件</Trans>
            </span>
          </div>
          <div className="flex flex-col items-center gap-0.5">
            <span className="font-mono text-lg font-bold tabular-nums text-foreground md:text-xl">
              {formatFileSize(projection.totalSize)}
            </span>
            <span className="text-[10px] text-muted-foreground md:text-[11px]">
              <Trans>总大小</Trans>
            </span>
          </div>
          <div className="flex flex-col items-center gap-0.5">
            <span className="font-mono text-lg font-bold tabular-nums text-foreground md:text-xl">
              {formatDuration(duration)}
            </span>
            <span className="text-[10px] text-muted-foreground md:text-[11px]">
              <Trans>用时</Trans>
            </span>
          </div>
        </div>
      </div>
    );
  }

  if (isProjectionFailed(projection)) {
    // 顶部徽章已显示具体状态（如「不可恢复失败」），这里给通用标题 + 人话原因 + 下一步，
    // 避免同一标签在面板里出现两次，也不把用户留在死胡同。
    return (
      <div className="flex flex-col items-center gap-2.5 py-2 md:gap-3 md:py-4">
        <div className="flex size-14 items-center justify-center rounded-full bg-red-100 dark:bg-red-500/15 md:size-16">
          <XCircle className="size-7 text-red-600 dark:text-red-400 md:size-8" />
        </div>
        <h3 className="text-base font-semibold text-foreground md:text-lg">
          <Trans>传输失败</Trans>
        </h3>
        {projection.errorMessage && (
          <p className="max-w-xs text-center text-xs leading-5 text-foreground/80 md:max-w-sm">
            {projection.errorMessage}
          </p>
        )}
        <p className="max-w-xs text-center text-[11px] text-muted-foreground md:max-w-sm md:text-xs">
          {projection.direction === "send" ? (
            <Trans>文件仍在本机，可以直接重新发送。</Trans>
          ) : (
            <Trans>接收未完成，请让对方重新发起传输。</Trans>
          )}
        </p>
      </div>
    );
  }

  if (projection.phase === "waiting_accept" || projection.phase === "offered") {
    return (
      <div className="flex flex-col items-center gap-2 py-2 md:py-4">
        <Loader2 className="size-6 animate-spin text-brand md:size-7" />
        <p className="text-[11px] text-muted-foreground md:text-xs">
          {projection.phase === "waiting_accept" ? (
            <Trans>等待对方确认...</Trans>
          ) : (
            <Trans>正在建立会话...</Trans>
          )}
        </p>
      </div>
    );
  }

  return null;
});

/* ─── 文件明细 ─── */

export const SessionFileSection = memo(function SessionFileSection({
  projection,
  progress,
  className,
}: {
  projection: TransferProjection;
  progress: TransferProgressEvent | null;
  className?: string;
}) {
  const treeData = useMemo(
    () => buildTreeDataFromSession({ files: projection.files }),
    [projection.files],
  );

  return (
    <div className={cn("flex min-h-0 flex-col gap-3", className)}>
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>文件明细</Trans>
        </h2>
        <span className="text-xs text-muted-foreground">
          {projection.files.length} <Trans>个文件</Trans>
        </span>
      </div>
      <FileTree
        mode={projection.phase === "active" ? "transfer" : "select"}
        dataLoader={treeData.dataLoader}
        rootChildren={treeData.rootChildren}
        totalCount={projection.files.length}
        totalSize={projection.totalSize}
        progress={progress}
        showHeader={false}
      />
    </div>
  );
});

/* ─── 操作按钮 ─── */

export const SessionActions = memo(function SessionActions({
  projection,
  onSessionChange,
  trailing,
}: {
  projection: TransferProjection;
  /** 恢复传输产生新会话 ID 时回调（用于更新选中态 / 跟踪目标） */
  onSessionChange?: (newSessionId: string) => void;
  /** 使用方追加的按钮（如发送流完成态的「继续发送」） */
  trailing?: React.ReactNode;
}) {
  const navigate = useNavigate();
  const isActive = isProjectionActive(projection);
  const isPaused = projection.phase === "suspended";
  const [isCancelling, setIsCancelling] = useState(false);
  const [isResending, setIsResending] = useState(false);

  const handlePause = useCallback(async () => {
    try {
      await doPauseTransfer(projection.sessionId);
    } catch {
      // doPauseTransfer 已 toast
    }
  }, [projection.sessionId]);

  const handleCancel = useCallback(async () => {
    if (isCancelling) return;
    setIsCancelling(true);
    try {
      await doCancelTransfer(projection.sessionId, projection.direction);
    } catch {
      // doCancelTransfer 已 toast
    } finally {
      setIsCancelling(false);
    }
  }, [isCancelling, projection.sessionId, projection.direction]);

  const handleOpenFolder = useCallback(async () => {
    try {
      await openTransferResult({
        saveLocation: projection.savePath ?? undefined,
        files: projection.files,
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  }, [projection.savePath, projection.files]);

  const handleResume = useCallback(async () => {
    try {
      const newSessionId = await doResumeTransfer(projection.sessionId);
      onSessionChange?.(newSessionId);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  }, [onSessionChange, projection.sessionId]);

  // 重新发送：doResendTransfer 取回源路径塞进 share-store（携带原目标设备），
  // 成功后导航到快捷发送流（导航留在组件，与 handleResume 同）。
  const handleResend = useCallback(async () => {
    setIsResending(true);
    try {
      await doResendTransfer(projection);
      void navigate({ to: "/send/share-target" });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setIsResending(false);
    }
  }, [navigate, projection]);

  return (
    <div className="flex flex-wrap items-center justify-end gap-2">
      {isPaused && canResumeProjection(projection) && (
        <Button
          onClick={handleResume}
          className="rounded-full px-5 shadow-[0_10px_22px_rgba(219,163,65,0.18)]"
        >
          <Play className="mr-2 size-4" />
          <Trans>恢复传输</Trans>
        </Button>
      )}

      {isActive && (
        <>
          {projection.phase === "active" && (
            <Button
              variant="secondary"
              onClick={handlePause}
              className="rounded-full px-5"
            >
              <Pause className="mr-2 size-4" />
              <Trans>暂停传输</Trans>
            </Button>
          )}
          <Button
            variant="outline"
            onClick={handleCancel}
            className="rounded-full px-5"
            disabled={isCancelling}
          >
            {isCancelling ? (
              <Loader2 className="mr-2 size-4 animate-spin" />
            ) : (
              <X className="mr-2 size-4" />
            )}
            {isCancelling ? <Trans>取消中...</Trans> : <Trans>取消传输</Trans>}
          </Button>
        </>
      )}

      {isProjectionCompleted(projection) && projection.savePath && (
        <Button
          onClick={handleOpenFolder}
          className="rounded-full px-5 shadow-[0_10px_22px_rgba(219,163,65,0.18)]"
        >
          <FolderOpen className="mr-2 size-4" />
          <Trans>打开文件夹</Trans>
        </Button>
      )}

      {canResendProjection(projection) && (
        <Button
          onClick={handleResend}
          disabled={isResending}
          className="rounded-full px-5 shadow-[0_10px_22px_rgba(219,163,65,0.18)]"
        >
          {isResending ? (
            <Loader2 className="mr-2 size-4 animate-spin" />
          ) : (
            <RotateCcw className="mr-2 size-4" />
          )}
          <Trans>重新发送</Trans>
        </Button>
      )}

      {trailing}
    </div>
  );
});
