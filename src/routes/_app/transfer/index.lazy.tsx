/**
 * Transfer Page (Lazy)
 * 活动与恢复页面 - 懒加载组件
 * 展示活跃传输、可恢复任务和过程账本诊断
 */

import { useEffect, useMemo, type ComponentType, type ReactNode } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import { Activity, ArrowLeftRight, CheckCircle2, RotateCcw, Trash2, TriangleAlert } from "lucide-react";
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
import { cn } from "@/lib/utils";

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
        <div className="mx-auto max-w-[1040px]">{content}</div>
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
      <div className="glass-panel rounded-[26px] p-2">
        <div className="grid gap-2 rounded-[20px] bg-white/30 p-2 shadow-[inset_0_1px_0_rgba(255,255,255,0.45)] dark:bg-white/[0.035] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)] sm:grid-cols-2 lg:grid-cols-4">
          <ActivityMetric
            icon={ArrowLeftRight}
            label={<Trans>活跃传输</Trans>}
            value={activeItems.length}
            tone="blue"
          />
          <ActivityMetric
            icon={RotateCcw}
            label={<Trans>可恢复</Trans>}
            value={recoveryItems.length}
            tone="green"
          />
          <ActivityMetric
            icon={TriangleAlert}
            label={<Trans>需要关注</Trans>}
            value={attentionItems.length}
            tone="amber"
          />
          <ActivityMetric
            icon={CheckCircle2}
            label={<Trans>已完成</Trans>}
            value={completedItems.length}
            tone="muted"
          />
        </div>
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

      {onClearActivity && (
        <div className="flex justify-end">
          <Button
            size="sm"
            variant="ghost"
            className="h-8 gap-1.5 rounded-full px-3 text-xs text-muted-foreground transition-[color,background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-destructive/10 hover:text-destructive active:scale-[0.98]"
            onClick={onClearActivity}
          >
            <Trash2 className="size-3.5" />
            <Trans>清空活动记录</Trans>
          </Button>
        </div>
      )}
    </div>
  );
}

function ActivityMetric({
  icon: Icon,
  label,
  value,
  tone,
}: {
  icon: ComponentType<{ className?: string }>;
  label: ReactNode;
  value: number;
  tone: "blue" | "green" | "amber" | "muted";
}) {
  return (
    <div className="glass-control rounded-[18px] px-3.5 py-3">
      <div className="flex items-center justify-between gap-3">
        <span
          className={cn(
            "flex size-8 items-center justify-center rounded-[12px]",
            tone === "blue" && "bg-blue-500/10 text-blue-600 dark:text-blue-300",
            tone === "green" && "bg-green-500/10 text-green-700 dark:text-green-300",
            tone === "amber" && "bg-amber-500/12 text-amber-700 dark:text-amber-300",
            tone === "muted" && "bg-foreground/[0.045] text-muted-foreground dark:bg-white/[0.06]",
          )}
        >
          <Icon className="size-4" />
        </span>
        <span className="font-mono text-2xl font-semibold tracking-tight text-foreground">
          {value}
        </span>
      </div>
      <p className="mt-2 text-xs text-muted-foreground">{label}</p>
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
    <section className="glass-panel rounded-[24px] p-2">
      <div className="rounded-[18px] bg-white/28 p-3 shadow-[inset_0_1px_0_rgba(255,255,255,0.42)] dark:bg-white/[0.03] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]">
        <div className="mb-3 flex items-center gap-2 text-muted-foreground">
          <span className="glass-control flex size-8 items-center justify-center rounded-full">
            {icon}
          </span>
          <h2 className="text-sm font-semibold text-foreground">{title}</h2>
        </div>
        {children}
      </div>
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
