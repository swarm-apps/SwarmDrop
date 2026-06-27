/**
 * Transfer Page (Lazy)
 * 传输页面 - 懒加载组件
 * 展示活跃传输和持久化历史记录
 */

import { useState, useEffect, useMemo } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import { ArrowLeftRight, Trash2 } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { useTransferStore } from "@/stores/transfer-store";
import { TransferItem } from "./-transfer-item";
import { HistoryItem } from "./-history-item";
import { commands, type TransferProjection } from "@/lib/bindings";
import {
  isProjectionActive,
  projectionMatchesFilter,
  type ProjectionStatusFilter,
} from "@/lib/transfer-projection";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
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

  const [statusFilter, setStatusFilter] =
    useState<ProjectionStatusFilter>("all");

  const statusFilters: { value: ProjectionStatusFilter; label: string }[] = useMemo(
    () => [
      { value: "all", label: t`全部` },
      { value: "completed", label: t`已完成` },
      { value: "suspended", label: t`可恢复` },
      { value: "failed", label: t`不可恢复失败` },
      { value: "cancelled", label: t`已取消` },
    ],
    [],
  );

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

  // 过滤历史投影：活跃列表已经展示的 session 不再重复出现在历史区
  const filteredHistory = useMemo(
    () =>
      projectionItems.filter(
        (item) =>
          !activeIdSet.has(item.sessionId) &&
          projectionMatchesFilter(item, statusFilter),
      ),
    [activeIdSet, projectionItems, statusFilter],
  );

  const hasContent = activeItems.length > 0 || filteredHistory.length > 0;

  const handleClearHistory = async () => {
    try {
      await commands.clearTransferHistory();
      await loadProjections();
      toast.success(t`已清空传输历史`);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };

  // 「传输历史」section 标题右侧的过滤 + 清空操作
  const historyToolbar = projectionItems.length > 0 && (
    <div className="flex items-center gap-1.5 md:gap-2">
      {/* 状态过滤 */}
      <Select
        value={statusFilter}
        onValueChange={(v) => setStatusFilter(v as ProjectionStatusFilter)}
      >
        <SelectTrigger className="h-7 w-auto gap-1 px-2 text-xs md:gap-1.5 md:px-2.5">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {statusFilters.map((f) => (
            <SelectItem key={f.value} value={f.value} className="text-xs">
              {f.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {/* 清空历史 */}
      <Button
        size="sm"
        variant="ghost"
        className="h-7 gap-1 px-2 text-xs text-muted-foreground hover:text-destructive"
        onClick={handleClearHistory}
      >
        <Trash2 className="size-3" />
        <span className="hidden md:inline"><Trans>清空</Trans></span>
      </Button>
    </div>
  );

  const content = !hasContent ? (
    <EmptyState />
  ) : (
    <TransferList
      activeItems={activeItems}
      historyItems={filteredHistory}
      historyToolbar={historyToolbar}
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
  historyItems,
  historyToolbar,
}: {
  activeItems: TransferProjection[];
  historyItems: TransferProjection[];
  historyToolbar: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-5">
      {/* 活跃传输 */}
      {activeItems.length > 0 && (
        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-foreground">
            <Trans>活跃传输</Trans>
          </h2>
          <div className="flex flex-col gap-2.5">
            {activeItems.map((item) => (
              <TransferItem key={item.sessionId} projection={item} />
            ))}
          </div>
        </section>
      )}

      {/* 传输历史(从 DB 加载) —— 过滤 / 清空挂在 section 标题右侧 */}
      {historyItems.length > 0 && (
        <section className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold text-foreground">
              <Trans>传输历史</Trans>
            </h2>
            {historyToolbar}
          </div>
          <div className="flex flex-col gap-2.5">
            {historyItems.map((item) => (
              <HistoryItem key={item.sessionId} item={item} />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

/* ─────────────────── 空状态 ─────────────────── */

function EmptyState() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
      <div className="flex size-14 items-center justify-center rounded-full bg-muted">
        <ArrowLeftRight className="size-7 text-muted-foreground" />
      </div>
      <div className="flex flex-col gap-1">
        <p className="text-sm font-medium text-foreground">
          <Trans>暂无传输记录</Trans>
        </p>
        <p className="text-xs text-muted-foreground">
          <Trans>在设备页面选择已配对设备发送文件</Trans>
        </p>
      </div>
    </div>
  );
}
