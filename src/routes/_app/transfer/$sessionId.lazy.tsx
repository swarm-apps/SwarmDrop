/**
 * Transfer Detail Page (Lazy)
 * 传输详情页面 - 展示传输进度、文件树、统计信息
 */

import { useMemo, memo, useCallback, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import {
  CheckCircle2,
  XCircle,
  FolderOpen,
  Loader2,
  X,
  Pause,
  Play,
} from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useTransferStore } from "@/stores/transfer-store";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { formatFileSize, formatSpeed, formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import { openTransferResult } from "@/lib/file-picker";
import { toast } from "sonner";
import { getErrorMessage } from "@/lib/errors";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { FileTree } from "@/components/file-tree";
import { buildTreeDataFromSession } from "@/components/file-tree";
import type { TransferProjection, TransferProgressEvent } from "@/lib/bindings";
import {
  calcPercent,
  projectionStatusClassName,
  projectionStatusLabel,
  doPauseTransfer,
  doCancelTransfer,
  doResumeTransfer,
} from "./-shared";
import {
  canResumeProjection,
  isProjectionActive,
  isProjectionCompleted,
  isProjectionFailed,
} from "@/lib/transfer-projection";
import {
  CommandDock,
  GlassPanel,
  TaskContent,
  TaskPageShell,
  TaskToolbar,
} from "@/components/layout/task-surface";

export const Route = createLazyFileRoute("/_app/transfer/$sessionId")({
  component: TransferDetailPage,
});

function TransferDetailPage() {
  const { sessionId } = Route.useParams();
  const navigate = useNavigate();

  const projection = useTransferStore(
    useCallback((s) => s.projections[sessionId], [sessionId]),
  );
  const progress = useTransferStore(
    useCallback((s) => s.progressBySession[sessionId] ?? null, [sessionId]),
  );

  const handleBack = useCallback(() => {
    navigate({ to: "/transfer" });
  }, [navigate]);

  if (!projection) {
    return (
      <main className="flex h-full flex-col items-center justify-center gap-3">
        <p className="text-sm text-muted-foreground">
          <Trans>传输记录不存在</Trans>
        </p>
        <Button variant="outline" onClick={handleBack}>
          <Trans>返回</Trans>
        </Button>
      </main>
    );
  }

  return (
    <TransferDetailContent
      projection={projection}
      progress={progress}
      onBack={handleBack}
    />
  );
}

/* ─────────────────── 共享组件 ─────────────────── */

const TransferStatusHeader = memo(function TransferStatusHeader({
  projection,
}: {
  projection: TransferProjection;
}) {
  const isSend = projection.direction === "send";
  const isActive = isProjectionActive(projection);

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
      {/* 设备平台图标 */}
      <div
        className={cn(
          "flex size-11 shrink-0 items-center justify-center rounded-full md:size-12",
          isSend
            ? "bg-blue-50 dark:bg-blue-500/15"
            : "bg-green-50 dark:bg-green-500/15",
        )}
      >
        <DeviceIcon
          className={cn(
            "size-5.5 md:size-6",
            isSend
              ? "text-blue-600 dark:text-blue-400"
              : "text-green-600 dark:text-green-400",
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
      {isActive && <StatusBadge projection={projection} />}
    </div>
  );
});

const StatusBadge = memo(function StatusBadge({
  projection,
}: {
  projection: TransferProjection;
}) {
  return (
    <span
      className={cn(
        "shrink-0 rounded-full px-2 py-0.5 text-xs font-medium md:px-2.5",
        projectionStatusClassName(projection),
      )}
    >
      {projectionStatusLabel(projection)}
    </span>
  );
});

const TransferProgress = memo(function TransferProgress({
  projection,
  progress,
}: {
  projection: TransferProjection;
  progress: TransferProgressEvent | null;
}) {
  const progressPercent = progress
    ? calcPercent(progress.transferredBytes, progress.totalBytes)
    : 0;

  // 来自投影的可恢复 suspended 会话
  if (projection.phase === "suspended") {
    const pausedPercent = calcPercent(
      projection.transferredBytes ?? 0,
      projection.totalSize,
    );
    return (
      <div className="flex flex-col gap-2 md:gap-2.5">
        <div className="flex items-baseline justify-between">
          <span className="text-2xl font-bold tabular-nums text-foreground md:text-3xl">
            {pausedPercent}%
          </span>
          <span className="text-[11px] text-muted-foreground md:text-xs">
            {projectionStatusLabel(projection)}
          </span>
        </div>
        <Progress value={pausedPercent} className="h-1.5 md:h-2" />
        <div className="flex items-center justify-between text-[11px] text-muted-foreground md:text-xs">
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
          <span className="text-2xl font-bold tabular-nums text-foreground md:text-3xl">
            {progressPercent}%
          </span>
          <span className="text-[11px] text-muted-foreground md:text-xs">
            {formatSpeed(progress.speed)}
          </span>
        </div>
        <Progress value={progressPercent} className="h-1.5 md:h-2" />
        <div className="flex items-center justify-between text-[11px] text-muted-foreground md:text-xs">
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

        <div className="flex w-full max-w-xs justify-between px-4 md:max-w-sm md:gap-8 md:justify-center md:px-0">
          <div className="flex flex-col items-center gap-0.5">
            <span className="text-lg font-bold tabular-nums text-foreground md:text-xl">
              {projection.files.length}
            </span>
            <span className="text-[10px] text-muted-foreground md:text-[11px]">
              <Trans>文件</Trans>
            </span>
          </div>
          <div className="flex flex-col items-center gap-0.5">
            <span className="text-lg font-bold tabular-nums text-foreground md:text-xl">
              {formatFileSize(projection.totalSize)}
            </span>
            <span className="text-[10px] text-muted-foreground md:text-[11px]">
              <Trans>总大小</Trans>
            </span>
          </div>
          <div className="flex flex-col items-center gap-0.5">
            <span className="text-lg font-bold tabular-nums text-foreground md:text-xl">
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
    return (
      <div className="flex flex-col items-center gap-2.5 py-2 md:gap-3 md:py-4">
        <div className="flex size-14 items-center justify-center rounded-full bg-red-100 dark:bg-red-500/15 md:size-16">
          <XCircle className="size-7 text-red-600 dark:text-red-400 md:size-8" />
        </div>
        <h3 className="text-base font-semibold text-foreground md:text-lg">
          {projectionStatusLabel(projection)}
        </h3>
        {projection.errorMessage && (
          <p className="max-w-xs text-center text-[11px] text-muted-foreground md:max-w-sm md:text-xs">
            {projection.errorMessage}
          </p>
        )}
      </div>
    );
  }

  if (projection.phase === "waiting_accept") {
    return (
      <div className="flex flex-col items-center gap-2 py-2 md:py-4">
        <Loader2 className="size-6 animate-spin text-primary md:size-7" />
        <p className="text-[11px] text-muted-foreground md:text-xs">
          <Trans>等待对方确认...</Trans>
        </p>
      </div>
    );
  }

  return null;
});

const TransferActions = memo(function TransferActions({
  projection,
}: {
  projection: TransferProjection;
}) {
  const isSend = projection.direction === "send";
  const isActive = isProjectionActive(projection);
  const isPaused = projection.phase === "suspended";
  const navigate = useNavigate();
  const [isCancelling, setIsCancelling] = useState(false);

  const handlePause = useCallback(async () => {
    try {
      await doPauseTransfer(projection.sessionId);
      navigate({ to: "/transfer" });
    } catch {
      // doPauseTransfer 已 toast
    }
  }, [projection.sessionId, navigate]);

  const handleCancel = useCallback(async () => {
    if (isCancelling) return;
    setIsCancelling(true);
    try {
      await doCancelTransfer(projection.sessionId, isSend ? "send" : "receive");
      navigate({ to: "/transfer" });
    } catch {
      setIsCancelling(false);
    }
  }, [isCancelling, isSend, navigate, projection.sessionId]);

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
      navigate({
        to: "/transfer/$sessionId",
        params: { sessionId: newSessionId },
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  }, [navigate, projection.sessionId]);

  if (isPaused && canResumeProjection(projection)) {
    return (
      <Button onClick={handleResume} className="rounded-full bg-blue-600 px-5 text-white shadow-[0_10px_22px_rgba(37,99,235,0.18)] hover:bg-blue-700">
        <Play className="mr-2 size-4" />
        <Trans>恢复传输</Trans>
      </Button>
    );
  }

  if (isActive) {
    return (
      <div className="flex gap-2">
        {projection.phase === "active" && (
          <Button variant="secondary" onClick={handlePause} className="rounded-full px-5">
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
      </div>
    );
  }

  if (isProjectionCompleted(projection) && projection.savePath) {
    return (
      <Button onClick={handleOpenFolder} className="rounded-full bg-blue-600 px-5 text-white shadow-[0_10px_22px_rgba(37,99,235,0.18)] hover:bg-blue-700">
        <FolderOpen className="mr-2 size-4" />
        <Trans>打开文件夹</Trans>
      </Button>
    );
  }

  return null;
});

/* ─────────────────── 统一详情视图 ─────────────────── */

const TransferDetailContent = memo(function TransferDetailContent({
  projection,
  progress,
  onBack,
}: {
  projection: TransferProjection;
  progress: TransferProgressEvent | null;
  onBack: () => void;
}) {
  const treeData = useMemo(() => {
    return buildTreeDataFromSession({ files: projection.files });
  }, [projection.files]);

  return (
    <TaskPageShell>
      <TaskToolbar title={<Trans>传输详情</Trans>} onBack={onBack} />

      <TaskContent className="flex min-h-0 flex-col gap-5">
        <GlassPanel>
          <div className="grid gap-5 p-5 lg:grid-cols-[minmax(0,1fr)_320px] lg:p-6">
            <TransferStatusHeader projection={projection} />
            <TransferProgress projection={projection} progress={progress} />
          </div>
        </GlassPanel>

        <GlassPanel className="min-h-0 flex-1">
          <div className="flex h-full min-h-0 flex-col gap-3 p-4 lg:p-5">
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
        </GlassPanel>

        <CommandDock>
          <TransferActions projection={projection} />
        </CommandDock>
      </TaskContent>
    </TaskPageShell>
  );
});
