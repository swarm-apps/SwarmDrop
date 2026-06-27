/**
 * Transfer Detail Page (Lazy)
 * 传输详情页面 - 展示传输进度、文件树、统计信息
 */

import { useMemo, memo, useCallback, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import {
  ArrowLeft,
  CheckCircle2,
  XCircle,
  FolderOpen,
  Loader2,
  X,
  Pause,
  Play,
} from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
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
import type { TransferProjection } from "@/lib/bindings";
import type { TransferSession } from "@/lib/types";
import {
  calcPercent,
  isActiveStatus,
  STATUS_CLASSNAMES,
  projectionStatusLabel,
  projectionToSession,
  doPauseTransfer,
  doCancelTransfer,
  doResumeTransfer,
} from "./-shared";
import { canResumeProjection } from "@/lib/transfer-projection";

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
  const session = useMemo(
    () => (projection ? projectionToSession(projection, progress) : undefined),
    [progress, projection],
  );

  const handleBack = useCallback(() => {
    navigate({ to: "/transfer" });
  }, [navigate]);

  if (!session) {
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
      session={session}
      projection={projection}
      onBack={handleBack}
    />
  );
}

/* ─────────────────── 共享组件 ─────────────────── */

const TransferStatusHeader = memo(function TransferStatusHeader({
  session,
  projection,
}: {
  session: TransferSession;
  projection?: TransferProjection;
}) {
  const isSend = session.direction === "send";
  const isActive = isActiveStatus(session.status);

  // 从 network store / secret store 查找设备 OS
  const deviceOs = useNetworkStore(
    (s) => s.devices.find((d) => d.peerId === session.peerId)?.os,
  );
  const pairedOs = useSecretStore(
    (s) => s.pairedDevices.find((d) => d.peerId === session.peerId)?.os,
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
          {session.deviceName}
        </h2>
        <p className="text-[11px] text-muted-foreground md:text-xs">
          {isSend ? <Trans>发送</Trans> : <Trans>接收</Trans>}
          {" · "}
          {session.files.length} <Trans>个文件</Trans>
          {" · "}
          {formatFileSize(session.totalSize)}
        </p>
      </div>
      {isActive && <StatusBadge status={session.status} projection={projection} />}
    </div>
  );
});

const StatusBadge = memo(function StatusBadge({
  status,
  projection,
}: {
  status: TransferSession["status"];
  projection?: TransferProjection;
}) {
  const labels: Record<TransferSession["status"], string> = {
    pending: t`等待中`,
    waiting_accept: t`等待确认`,
    transferring: t`传输中`,
    paused: t`已暂停`,
    completed: t`已完成`,
    failed: t`失败`,
    cancelled: t`已取消`,
  };

  return (
    <span
      className={cn(
        "shrink-0 rounded-full px-2 py-0.5 text-xs font-medium md:px-2.5",
        STATUS_CLASSNAMES[status],
      )}
    >
      {projection ? projectionStatusLabel(projection) : labels[status]}
    </span>
  );
});

const TransferProgress = memo(function TransferProgress({
  session,
  projection,
}: {
  session: TransferSession;
  projection?: TransferProjection;
}) {
  const progressPercent = session.progress
    ? calcPercent(
        session.progress.transferredBytes,
        session.progress.totalBytes,
      )
    : 0;

  // 来自投影的可恢复 suspended 会话
  if (session.status === "paused") {
    const pausedPercent = calcPercent(
      session.transferredBytes ?? 0,
      session.totalSize,
    );
    return (
      <div className="flex flex-col gap-2 md:gap-2.5">
        <div className="flex items-baseline justify-between">
          <span className="text-2xl font-bold tabular-nums text-foreground md:text-3xl">
            {pausedPercent}%
          </span>
          <span className="text-[11px] text-muted-foreground md:text-xs">
            {projection ? projectionStatusLabel(projection) : t`已暂停`}
          </span>
        </div>
        <Progress value={pausedPercent} className="h-1.5 md:h-2" />
        <div className="flex items-center justify-between text-[11px] text-muted-foreground md:text-xs">
          <span>
            {formatFileSize(session.transferredBytes ?? 0)} /{" "}
            {formatFileSize(session.totalSize)}
          </span>
        </div>
      </div>
    );
  }

  if (session.status === "transferring" && session.progress) {
    return (
      <div className="flex flex-col gap-2 md:gap-2.5">
        <div className="flex items-baseline justify-between">
          <span className="text-2xl font-bold tabular-nums text-foreground md:text-3xl">
            {progressPercent}%
          </span>
          <span className="text-[11px] text-muted-foreground md:text-xs">
            {formatSpeed(session.progress.speed)}
          </span>
        </div>
        <Progress value={progressPercent} className="h-1.5 md:h-2" />
        <div className="flex items-center justify-between text-[11px] text-muted-foreground md:text-xs">
          <span>
            {formatFileSize(session.progress.transferredBytes)} /{" "}
            {formatFileSize(session.progress.totalBytes)}
          </span>
          {session.progress.eta != null && (
            <span>
              <Trans>剩余 {formatDuration(session.progress.eta)}</Trans>
            </span>
          )}
        </div>
      </div>
    );
  }

  if (session.status === "completed") {
    const duration = session.completedAt
      ? Math.round((session.completedAt - session.startedAt) / 1000)
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
              {session.files.length}
            </span>
            <span className="text-[10px] text-muted-foreground md:text-[11px]">
              <Trans>文件</Trans>
            </span>
          </div>
          <div className="flex flex-col items-center gap-0.5">
            <span className="text-lg font-bold tabular-nums text-foreground md:text-xl">
              {formatFileSize(session.totalSize)}
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

  if (session.status === "failed") {
    return (
      <div className="flex flex-col items-center gap-2.5 py-2 md:gap-3 md:py-4">
        <div className="flex size-14 items-center justify-center rounded-full bg-red-100 dark:bg-red-500/15 md:size-16">
          <XCircle className="size-7 text-red-600 dark:text-red-400 md:size-8" />
        </div>
        <h3 className="text-base font-semibold text-foreground md:text-lg">
          {projection ? projectionStatusLabel(projection) : <Trans>传输失败</Trans>}
        </h3>
        {session.error && (
          <p className="max-w-xs text-center text-[11px] text-muted-foreground md:max-w-sm md:text-xs">
            {session.error}
          </p>
        )}
      </div>
    );
  }

  if (session.status === "waiting_accept") {
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
  session,
  projection,
}: {
  session: TransferSession;
  projection?: TransferProjection;
}) {
  const isSend = session.direction === "send";
  const isActive = isActiveStatus(session.status);
  const isPaused = session.status === "paused";
  const navigate = useNavigate();
  const [isCancelling, setIsCancelling] = useState(false);

  const handlePause = useCallback(async () => {
    try {
      await doPauseTransfer(session.sessionId);
      navigate({ to: "/transfer" });
    } catch {
      // doPauseTransfer 已 toast
    }
  }, [session.sessionId, navigate]);

  const handleCancel = useCallback(async () => {
    if (isCancelling) return;
    setIsCancelling(true);
    try {
      await doCancelTransfer(session.sessionId, isSend ? "send" : "receive");
      navigate({ to: "/transfer" });
    } catch {
      setIsCancelling(false);
    }
  }, [isCancelling, isSend, navigate, session.sessionId]);

  const handleOpenFolder = useCallback(async () => {
    try {
      await openTransferResult(session);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  }, [session]);

  const handleResume = useCallback(async () => {
    if (!projection) return;
    try {
      const newSessionId = await doResumeTransfer(projection.sessionId);
      navigate({
        to: "/transfer/$sessionId",
        params: { sessionId: newSessionId },
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  }, [navigate, projection]);

  if (isPaused && projection && canResumeProjection(projection)) {
    return (
      <Button onClick={handleResume} className="w-full">
        <Play className="mr-2 size-4" />
        <Trans>恢复传输</Trans>
      </Button>
    );
  }

  if (isActive) {
    return (
      <div className="flex w-full gap-2">
        {session.status === "transferring" && (
          <Button variant="secondary" onClick={handlePause} className="flex-1">
            <Pause className="mr-2 size-4" />
            <Trans>暂停传输</Trans>
          </Button>
        )}
        <Button
          variant="outline"
          onClick={handleCancel}
          className="flex-1"
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

  if (session.status === "completed" && session.saveLocation) {
    return (
      <Button onClick={handleOpenFolder} className="w-full">
        <FolderOpen className="mr-2 size-4" />
        <Trans>打开文件夹</Trans>
      </Button>
    );
  }

  return null;
});

/* ─────────────────── 统一详情视图 ─────────────────── */

const TransferDetailContent = memo(function TransferDetailContent({
  session,
  projection,
  onBack,
}: {
  session: TransferSession;
  projection?: TransferProjection;
  onBack: () => void;
}) {
  const treeData = useMemo(() => {
    return buildTreeDataFromSession(session);
  }, [session]);

  return (
    <main className="flex h-full flex-1 flex-col bg-transparent">
      {/* 头部 */}
      <header className="flex h-13 items-center gap-2 border-b border-border px-4 lg:px-5">
        <button
          type="button"
          onClick={onBack}
          className="flex size-8 items-center justify-center rounded-md hover:bg-muted"
        >
          <ArrowLeft className="size-4" />
        </button>
        <h1 className="min-w-0 truncate text-sm font-medium text-foreground">
          <Trans>传输详情</Trans>
        </h1>
      </header>

      {/* 内容 */}
      <div className="flex-1 overflow-auto p-4 lg:p-5">
        <div className="mx-auto flex max-w-2xl flex-col gap-5">
          <TransferStatusHeader session={session} projection={projection} />

          <TransferProgress session={session} projection={projection} />

          <div className="flex flex-col gap-2">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              <Trans>传输详情</Trans>
            </h3>
            <FileTree
              mode={session.status === "transferring" ? "transfer" : "select"}
              dataLoader={treeData.dataLoader}
              rootChildren={treeData.rootChildren}
              totalCount={session.files.length}
              totalSize={session.totalSize}
              progress={session.progress}
            />
          </div>

          <div className="flex justify-end">
            <TransferActions session={session} projection={projection} />
          </div>
        </div>
      </div>
    </main>
  );
});
