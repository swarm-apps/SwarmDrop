import {
  CheckCircle2,
  XCircle,
  Pause,
  Play,
  Trash2,
  FileArchive,
  FolderOpen,
} from "lucide-react";
import { Trans, useLingui } from "@lingui/react/macro";
import type { TransferProjection } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { formatFileSize, formatRelativeTime } from "@/lib/format";
import { getFileIcon, getFileIconColor } from "@/lib/file-icon";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { PolicyReasonBadge } from "@/components/transfer/policy-reason-badge";
import { toast } from "sonner";
import { getErrorMessage } from "@/lib/errors";
import { useTransferStore } from "@/stores/transfer-store";
import { useNavigate } from "@tanstack/react-router";
import { openTransferResult } from "@/lib/file-picker";
import {
  DirectionIcon,
  TransferCard,
  calcPercent,
  projectionStatusLabel,
  doResumeTransfer,
  ACTION_BTN_CLASS,
  DESTRUCTIVE_BTN_CLASS,
} from "./-shared";
import {
  canResumeProjection,
  isProjectionCompleted,
  isProjectionCancelled,
  isProjectionFailed,
} from "@/lib/transfer-projection";

interface HistoryItemProps {
  item: TransferProjection;
}

/** 辅助函数：截断 PeerId 以美化显示 */
const truncatePeerId = (id?: string) =>
  !id ? "" : id.length <= 16 ? id : `${id.slice(0, 8)}...${id.slice(-4)}`;

/** 文件类型图标（多文件聚合成压缩包图标，否则按扩展名取图标） */
function FileTypeIcon({ name, count }: { name: string; count: number }) {
  if (count > 1) return <FileArchive className="size-5 text-amber-500" />;
  const Icon = getFileIcon(name);
  return <Icon className={`size-5 ${getFileIconColor(name)}`} />;
}

export function HistoryItem({ item }: HistoryItemProps) {
  const { t } = useLingui();
  const navigate = useNavigate();
  const { loadProjections } = useTransferStore();

  const {
    sessionId,
    direction,
    peerId,
    peerName,
    files,
    totalSize,
    errorMessage,
    startedAt,
    finishedAt,
    transferredBytes,
  } = item;
  const statusLabel = projectionStatusLabel(item);
  const isCompleted = isProjectionCompleted(item);
  const isSuspended = item.phase === "suspended";
  const isFailed = isProjectionFailed(item);
  const isCancelled = isProjectionCancelled(item);

  // 事件处理
  const withAction =
    (action: () => Promise<void>) => async (e: React.MouseEvent) => {
      e.stopPropagation();
      try {
        await action();
      } catch (err) {
        toast.error(getErrorMessage(err));
      }
    };

  const onResume = withAction(async () => {
    const newSessionId = await doResumeTransfer(sessionId);
    navigate({
      to: "/transfer/$sessionId",
      params: { sessionId: newSessionId },
    });
  });

  const onDelete = withAction(async () => {
    await commands.deleteTransferSession(sessionId);
    await loadProjections();
  });

  const onOpenFolder = withAction(async () => {
    if (!item.savePath) return;
    await openTransferResult({
      saveLocation: item.savePath ?? undefined,
      files: files.map((f) => ({ relativePath: f.relativePath })),
    });
  });

  const handleClick = () => {
    navigate({
      to: "/transfer/$sessionId",
      params: { sessionId },
    });
  };

  // 计算数据
  const isSend = direction === "send";
  const fileCount = files?.length || 0;
  const firstFileName = files?.[0]?.name || t`未知文件`;
  const displayFileName =
    fileCount > 1 ? t`${firstFileName} 等 ${fileCount} 个文件` : firstFileName;
  const progressPercent = calcPercent(transferredBytes, totalSize);
  const canResume = canResumeProjection(item);

  return (
    <TransferCard onClick={handleClick}>
      <DirectionIcon isSend={isSend} />

      {/* 中间：核心信息 */}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5 md:gap-1">
        <div className="flex items-center gap-1.5 md:gap-2">
          <span className="hidden md:inline-flex">
            <FileTypeIcon name={firstFileName} count={fileCount} />
          </span>
          <h3
            className="truncate text-[13px] font-medium text-foreground md:text-sm"
            title={displayFileName}
          >
            {displayFileName}
          </h3>
        </div>

        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground md:text-xs">
          <span className="shrink-0">
            {isSend ? <Trans>发送到</Trans> : <Trans>来自</Trans>}
          </span>
          <span className="max-w-[6em] truncate font-medium text-foreground/80 md:max-w-[10em]">
            {peerName || truncatePeerId(peerId)}
          </span>
          <span className="shrink-0 text-muted-foreground/40">·</span>
          <span className="shrink-0">{formatFileSize(totalSize)}</span>
        </div>

        {/* 状态栏 */}
        <div className="mt-0.5">
          {isCompleted && (
            <div className="flex items-center gap-1.5 text-[12px] text-green-600 dark:text-green-400 md:text-[13px]">
              <CheckCircle2 className="size-3.5 md:size-4" />
              <Trans>传输完成</Trans>
              <span className="text-muted-foreground">
                — {formatFileSize(transferredBytes)}
              </span>
            </div>
          )}

          {isSuspended && (
            <div className="flex flex-col gap-1.5 mt-0.5">
              <Progress value={progressPercent} className="h-1.5" />
              <div className="flex items-center justify-between text-[11px] md:text-[12px]">
                <span className="flex items-center gap-1 text-amber-600 dark:text-amber-400">
                  <Pause className="size-3 md:size-3.5" />
                  {statusLabel}
                </span>
                <span className="text-muted-foreground">
                  {formatFileSize(transferredBytes)} /{" "}
                  {formatFileSize(totalSize)} · {progressPercent}%
                </span>
              </div>
            </div>
          )}

          {isFailed && (
            <div className="flex items-center gap-1.5 text-[12px] text-destructive md:text-[13px]">
              <XCircle className="size-3.5 shrink-0 md:size-4" />
              <span className="truncate">{errorMessage || statusLabel}</span>
            </div>
          )}

          {isCancelled && (
            <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground md:text-[13px]">
              <XCircle className="size-3.5 md:size-4" />
              {statusLabel}
            </div>
          )}

          <PolicyReasonBadge
            policyAction={item.policyAction}
            policyReason={item.policyReason}
          />
        </div>
      </div>

      {/* 右侧：时间 + 操作按钮 */}
      <div className="flex shrink-0 flex-col items-end gap-1 -mr-1 md:-mr-1.5">
        <span className="text-[10px] text-muted-foreground md:text-[11px]">
          {formatRelativeTime(finishedAt || startedAt)}
        </span>

        <div className="flex items-center gap-0.5">
          {canResume && (
            <Button
              size="icon"
              variant="ghost"
              className={ACTION_BTN_CLASS}
              onClick={onResume}
              title={t`恢复传输`}
            >
              <Play className="size-3.5 md:size-4" />
            </Button>
          )}
          {isCompleted && item.savePath && (
            <Button size="icon" variant="ghost" className={ACTION_BTN_CLASS} onClick={onOpenFolder} title={t`打开文件夹`}>
              <FolderOpen className="size-3.5 md:size-4" />
            </Button>
          )}
          <Button size="icon" variant="ghost" className={DESTRUCTIVE_BTN_CLASS} onClick={onDelete} title={t`删除记录`}>
            <Trash2 className="size-3.5 md:size-4" />
          </Button>
        </div>
      </div>
    </TransferCard>
  );
}
