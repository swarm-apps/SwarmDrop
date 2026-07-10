/**
 * SendProgressView
 * 发送流「就地转进度」视图 —— startSend 成功后，/send 与 /send/share-target
 * 不再跳独立详情页，而是原地切换到这个视图：进度 / 等待确认 / 完成 / 失败
 * 全程同一界面，右键快捷发送场景发完即可关窗。
 */

import { Check, Loader2, PackagePlus } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { useSessionProgress, useTransferStore } from "@/stores/transfer-store";
import {
  SessionActions,
  SessionFileSection,
  SessionProgressBlock,
  SessionSummaryHeader,
} from "@/components/transfer/session-panel";
import {
  isProjectionActive,
  isProjectionCompleted,
} from "@/lib/transfer-projection";
import {
  CommandDock,
  GlassPanel,
  TaskButton,
  TaskContent,
  TaskPageShell,
  TaskToolbar,
} from "@/components/layout/task-surface";

export function SendProgressView({
  sessionId,
  onBack,
  onSessionChange,
  onSendMore,
}: {
  sessionId: string;
  onBack: () => void;
  /** 恢复传输产生新会话时同步页面持有的 sessionId */
  onSessionChange: (newSessionId: string) => void;
  /** 「发送更多」：清空选择回到选文件视图（share-target 文件来自外部，不提供） */
  onSendMore?: () => void;
}) {
  const projection = useTransferStore((s) => s.projections[sessionId]);
  const progress = useSessionProgress(sessionId);

  if (!projection) {
    return (
      <TaskPageShell data-testid="send-progress-view">
        <TaskToolbar title={<Trans>发送文件</Trans>} onBack={onBack} />
        <TaskContent className="flex min-h-0 flex-col items-center justify-center">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </TaskContent>
      </TaskPageShell>
    );
  }

  const isTerminal = projection.phase === "terminal";
  const completed = isProjectionCompleted(projection);

  return (
    <TaskPageShell data-testid="send-progress-view">
      <TaskToolbar
        title={<Trans>发送到 {projection.peerName}</Trans>}
        onBack={onBack}
      />

      <TaskContent className="flex min-h-0 flex-col gap-5">
        <GlassPanel>
          <div className="grid gap-5 p-5 lg:grid-cols-[minmax(0,1fr)_320px] lg:p-6">
            <SessionSummaryHeader projection={projection} />
            <SessionProgressBlock projection={projection} progress={progress} />
          </div>
        </GlassPanel>

        <GlassPanel className="min-h-0 flex-1">
          <div className="flex h-full min-h-0 flex-col p-4 lg:p-5">
            <SessionFileSection
              projection={projection}
              progress={progress}
              className="h-full"
            />
          </div>
        </GlassPanel>

        <CommandDock>
          {/* 传输进行/暂停期间离开不打断：后台继续，可回活动中心查看 */}
          {isProjectionActive(projection) && (
            <p className="mr-auto hidden px-2 text-xs text-muted-foreground sm:block">
              <Trans>离开此页不会中断传输，可在「传输活动」中继续查看。</Trans>
            </p>
          )}
          <SessionActions
            projection={projection}
            onSessionChange={onSessionChange}
            trailing={
              isTerminal ? (
                <>
                  {onSendMore && (
                    <TaskButton variant="outline" onClick={onSendMore}>
                      <PackagePlus className="size-4" />
                      <Trans>发送更多</Trans>
                    </TaskButton>
                  )}
                  <TaskButton
                    variant={
                      completed && !projection.savePath ? "default" : "outline"
                    }
                    onClick={onBack}
                  >
                    <Check className="size-4" />
                    <Trans>完成</Trans>
                  </TaskButton>
                </>
              ) : null
            }
          />
        </CommandDock>
      </TaskContent>
    </TaskPageShell>
  );
}
