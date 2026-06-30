/**
 * Transfer Page (Lazy)
 * 活动与恢复页面 - 懒加载组件
 * 展示活跃传输、可恢复任务和过程账本诊断
 */

import { useEffect, useMemo } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import { Activity, ArrowLeftRight, RotateCcw, Trash2, TriangleAlert } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { useTransferStore } from "@/stores/transfer-store";
import { TransferItem } from "./-transfer-item";
import { HistoryItem } from "./-history-item";
import { commands, type TransferProjection } from "@/lib/bindings";
import {
  canResumeProjection,
  isProjectionActive,
  isProjectionCancelled,
  isProjectionCompleted,
  isProjectionFailed,
} from "@/lib/transfer-projection";
import { Button } from "@/components/ui/button";
import { CenteredEmptyState } from "@/components/layout/section-primitives";
import { toast } from "sonner";
import { getErrorMessage } from "@/lib/errors";

export const Route = createLazyFileRoute("/_app/transfer/")({
  component: TransferPage,
});

function TransferPage() {
  const projections = useTransferStore((s) => s.projections);
  const loadProjections = useTransferStore((s) => s.loadProjections);

  // 进入传输列表页时主动刷新后端 projection
  useEffect(() => {
    loadProjections();
  }, [loadProjections]);

  const projectionItems = useMemo(
    () =>
      Object.values(projections).sort((a, b) => b.startedAt - a.startedAt),
    [projections],
  );

  const activeItems = useMemo(
    () => projectionItems.filter(isProjectionActive),
    [projectionItems],
  );

  const activeIdSet = useMemo(
    () => new Set(activeItems.map((item) => item.sessionId)),
    [activeItems],
  );

  const recoveryItems = useMemo(
    () =>
      projectionItems.filter(
        (item) => !activeIdSet.has(item.sessionId) && canResumeProjection(item),
      ),
    [activeIdSet, projectionItems],
  );

  const attentionItems = useMemo(
    () =>
      projectionItems.filter((item) => {
        if (activeIdSet.has(item.sessionId) || canResumeProjection(item)) {
          return false;
        }
        if (item.phase === "suspended") return true;
        return isProjectionFailed(item) || isProjectionCancelled(item);
      }),
    [activeIdSet, projectionItems],
  );

  const completedItems = useMemo(
    () =>
      projectionItems.filter(
        (item) =>
          !activeIdSet.has(item.sessionId) && isProjectionCompleted(item),
      ),
    [activeIdSet, projectionItems],
  );

  const hasContent =
    activeItems.length > 0 ||
    recoveryItems.length > 0 ||
    attentionItems.length > 0 ||
    completedItems.length > 0;

  const handleClearHistory = async () => {
    try {
      await commands.clearTransferHistory();
      await loadProjections();
      toast.success(t`已清空活动记录`);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };

  const content = !hasContent ? (
    <EmptyState />
  ) : (
    <TransferList
      activeItems={activeItems}
      recoveryItems={recoveryItems}
      attentionItems={attentionItems}
      completedItems={completedItems}
      onClearActivity={projectionItems.length > 0 ? handleClearHistory : null}
    />
  );

  return (
    <main className="flex h-full flex-1 flex-col bg-transparent">
      {/* Page Content —— 页面标题由 AppTopBar 面包屑承担,无独立 header */}
      <div className="flex-1 overflow-auto p-5 lg:p-6">
        {content}
      </div>
    </main>
  );
}

/* ─────────────────── 传输列表 ─────────────────── */

function TransferList({
  activeItems,
  recoveryItems,
  attentionItems,
  completedItems,
  onClearActivity,
}: {
  activeItems: TransferProjection[];
  recoveryItems: TransferProjection[];
  attentionItems: TransferProjection[];
  completedItems: TransferProjection[];
  onClearActivity: (() => Promise<void>) | null;
}) {
  return (
    <div className="flex flex-col gap-5">
      <div className="flex items-center justify-end">
        {onClearActivity && (
          <Button
            size="sm"
            variant="ghost"
            className="h-8 gap-1.5 px-2.5 text-xs text-muted-foreground hover:text-destructive"
            onClick={onClearActivity}
          >
            <Trash2 className="size-3.5" />
            <Trans>清空活动记录</Trans>
          </Button>
        )}
      </div>

      {/* 活跃传输 */}
      {activeItems.length > 0 && (
        <ActivitySection
          title={<Trans>活跃传输</Trans>}
          icon={<ArrowLeftRight className="size-4" />}
        >
          <div className="flex flex-col gap-2.5">
            {activeItems.map((item) => (
              <TransferItem key={item.sessionId} projection={item} />
            ))}
          </div>
        </ActivitySection>
      )}

      {recoveryItems.length > 0 && (
        <ActivitySection
          title={<Trans>可恢复</Trans>}
          icon={<RotateCcw className="size-4" />}
        >
          <div className="flex flex-col gap-2.5">
            {recoveryItems.map((item) => (
              <HistoryItem key={item.sessionId} item={item} />
            ))}
          </div>
        </ActivitySection>
      )}

      {attentionItems.length > 0 && (
        <ActivitySection
          title={<Trans>需要关注</Trans>}
          icon={<TriangleAlert className="size-4" />}
        >
          <div className="flex flex-col gap-2.5">
            {attentionItems.map((item) => (
              <HistoryItem key={item.sessionId} item={item} />
            ))}
          </div>
        </ActivitySection>
      )}

      {completedItems.length > 0 && (
        <ActivitySection
          title={<Trans>完成诊断</Trans>}
          icon={<Activity className="size-4" />}
        >
          <div className="flex flex-col gap-2.5">
            {completedItems.map((item) => (
              <HistoryItem key={item.sessionId} item={item} />
            ))}
          </div>
        </ActivitySection>
      )}
    </div>
  );
}

function ActivitySection({
  title,
  icon,
  children,
}: {
  title: React.ReactNode;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-2 text-muted-foreground">
        {icon}
        <h2 className="text-sm font-semibold text-foreground">{title}</h2>
      </div>
      {children}
    </section>
  );
}

/* ─────────────────── 空状态 ─────────────────── */

function EmptyState() {
  return (
    <CenteredEmptyState
      icon={ArrowLeftRight}
      title={<Trans>暂无活动记录</Trans>}
      description={
        <Trans>成功接收的文件会进入收件箱，暂停或失败的任务会在这里恢复</Trans>
      }
    />
  );
}
