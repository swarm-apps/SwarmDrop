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
import {
  useSessionProgress,
  useSessionPublishing,
  useSessionRates,
  useTransferStore,
} from "@/stores/transfer-store";
import {
  calcPercent,
  formatFileSize,
  formatSpeed,
  relativeTimeMessage,
} from "@/lib/format";
import { failureCodeMessage, getErrorMessage } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { PolicyReasonBadge } from "@/components/transfer/policy-reason-badge";
import { DirectionIcon, EtaSlot } from "@/components/transfer/session-panel";
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
  // 速度与剩余时间同源同判：**不读 `progress.speed`**——那是最后一帧的原样值，停滞时后端的
  // 归零只对下一帧生效，而停滞恰恰意味着没有下一帧，这一格会一直挂着一个早已不成立的速率。
  const { eta, speed } = useSessionRates(sessionId);
  // 正在保存的文件。**不做成 prop**：本组件是 memo 且进度 200ms 一帧，父列表每帧新建的
  // 对象会直接打穿 memo；自己订阅拿到的是 store 里那份原引用。
  const publishing = useSessionPublishing(sessionId);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  const [isCancelling, setIsCancelling] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  // 统一的错误捕获 + toast，行内动作与弹窗确认共用（弹窗确认没有 MouseEvent）。
  const runSafe = async (action: () => Promise<void>) => {
    try {
      await action();
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };
  const withAction =
    (action: () => Promise<void>) => (e: React.MouseEvent) => {
      e.stopPropagation();
      void runSafe(action);
    };

  const onPause = withAction(() =>
    doPauseTransfer(sessionId, projection.direction),
  );
  const onResume = withAction(async () => {
    const newSessionId = await doResumeTransfer(sessionId);
    onSessionChange(newSessionId);
  });
  const handleDeleteConfirm = () => {
    setDeleteOpen(false);
    void runSafe(async () => {
      await commands.deleteTransferSession(sessionId);
      await loadProjections();
    });
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

        {/* 状态区。
            `@container`：速度那一格挂的是**容器**查询而不是视口断点——同一个 SessionRow
            既长在设备页的主栏里（宽 ~330–590px），也长在活动中心那条 300–360px 的会话列表
            里，而后者恰恰在「视口 ≥920」时才出现。按视口判会得到反的结果：宽窗口下最挤的
            那一栏反而把速度显示出来，把正在传输的文件名挤成一个省略号。
            这与设备卡片网格用 `auto-fill + minmax(300px,1fr)` 而不是 `xl:grid-cols-3`
            是同一条判据——列宽由这一栏能有多宽决定，不由窗口多宽决定。 */}
        <div className="@container mt-0.5">
          {/* **不要**把这里改回 `&& progress`：progress 还没到（冷启动首帧、或
              loadProjections 刚清过进度表）时整块状态区会变空白。旧排序下 active 行被
              phaseRank 钉在最顶，读者知道那一段是「进行中」；纯时间线下它可能落在任何
              位置，一行什么都不说的记录比排错位置更糟（Transfer List Order Contract：
              每行必须自述状态）。这里的每个值本来就有 progress 缺席时的退路：
              `progressPercent` 回落到 `transferredBytes / totalSize`，文件名回落到
              「传输中」，`formatSpeed` 与 `EtaSlot` 各自处理 undefined。 */}
          {projection.phase === "active" && (
            <div className="mt-0.5 flex flex-col gap-1.5">
              {/* 这里**刻意不给 `aria-label`**（详情面板那两条给了）：整行是
                  `role="button"`，而 ARIA 对 button 规定 Children Presentational: True
                  ——后代角色被辅助技术整个丢弃，写在这儿的可访问名不会「弱一点」，是
                  根本不生效，只会让维护者以为做过了。进度信息由行本身的可访问名承担
                  （名字从后代文本算出，百分比就在右边那格里）。 */}
              <Progress
                value={progressPercent}
                // 发布是纯本机拷贝 / 重命名，一个字节都不在网上——muted grey 的定义。
                tone={publishing ? "local" : "transfer"}
                className="h-1.5"
              />
              <div className="flex items-center justify-between gap-1.5 text-[11px]">
                <span
                  className={cn(
                    "flex min-w-0 items-center gap-1",
                    publishing ? "text-muted-foreground" : "text-brand",
                  )}
                >
                  <Loader2 className="size-3 shrink-0 animate-spin" />
                  <span className="truncate">
                    {publishing ? t`正在保存…` : activeFileName || t`传输中`}
                  </span>
                </span>
                {/* 等宽只包**机器值**（速度、百分比），不包整行：「剩余 1m 30s」
                    「计算中」是散文，CJK 落进等宽栈会回退到非等宽 CJK 字体、字距被撑开
                    （DESIGN.md 的 Mono Truth Rule 限定等宽用于可复制、可核对的字面值）。
                    `tabular-nums` 留在外层：它只管数字等宽，不换字体族。 */}
                <span className="shrink-0 tabular-nums text-muted-foreground">
                  {/* 发布期没有任何字节上路，速度与剩余时间都不成立——这一格只留百分比，
                      「在等什么」由左边那句话回答。 */}
                  {!publishing && (
                    <>
                      {/* 速度是四位里最不重要的一位：一行只放得下一个时 ETA 优先
                          （DESIGN.md 的 Transfer Progress Contract），所以放不下时藏掉的
                          是速度而不是剩余时间。
                          300 是量出来的，不是挑好看的：这一行满配是
                          「12.4 MB/s · 剩余 1m 30s · 47%」≈187px（11px 等宽 + 两个中文字），
                          再给文件名留 ~90px 才不至于只剩省略号 → 277，取 300 留余量。
                          **改这行的内容前先重新量**——多一个词就会把它顶过去，症状是
                          活动中心的文件名整列变成「…」。 */}
                      <span className="hidden @min-[300px]:inline">
                        <span className="font-mono">{formatSpeed(speed)}</span>
                        {" · "}
                      </span>
                      <EtaSlot eta={eta} />
                      {" · "}
                    </>
                  )}
                  <span className="font-mono">{progressPercent}%</span>
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
                  ? "text-warning-ink"
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
                <span className="flex items-center gap-1 text-warning-ink">
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
            <div className="flex items-center gap-1.5 text-[12px] text-success-ink">
              <CheckCircle2 className="size-3.5" />
              {projectionStatusLabel(projection)}
            </div>
          )}

          {isProjectionFailed(projection) && (
            <div className="flex items-center gap-1.5 text-[12px] text-destructive">
              <XCircle className="size-3.5 shrink-0" />
              <span className="truncate">
                {failureCodeMessage(projection.failure) ||
                  projectionStatusLabel(projection)}
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
        {/* 印的必须是排序键本身：按 updatedAt 排却印 startedAt，会让一条刚续传的会话
            置顶显示「3 天前」——行的文字和行的位置互相打脸。终态会话的 updatedAt 就是
            它进终态的那一刻，与 finishedAt 同义。 */}
        <span className="text-[10px] text-muted-foreground">
          {t(relativeTimeMessage(projection.updatedAt))}
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

      {/* 条件挂载：不常驻在高频进度重渲染的行子树里 */}
      {deleteOpen && (
        <ConfirmDialog
          open
          onOpenChange={setDeleteOpen}
          stopPropagation
          title={<Trans>删除「{displayFileName}」的传输记录？</Trans>}
          description={
            canResume ? (
              <Trans>
                删除后该任务的断点信息将一并清除，无法再继续续传；已传输的文件不受影响。若此刻它又恢复了传输，需先取消才能删除记录。
              </Trans>
            ) : (
              <Trans>
                记录删除后无法恢复；已传输的文件不受影响。若此刻它又恢复了传输，需先取消才能删除记录。
              </Trans>
            )
          }
          confirmLabel={<Trans>删除记录</Trans>}
          onConfirm={handleDeleteConfirm}
        />
      )}
    </div>
  );
});
