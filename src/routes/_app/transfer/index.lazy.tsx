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
import { useBreakpoint } from "@/hooks/use-breakpoint";
import { useTransferStore } from "@/stores/transfer-store";
import { TransferItem } from "./-transfer-item";
import { HistoryItem } from "./-history-item";
import type {
  TransferHistoryItem,
  HistorySessionStatus,
} from "@/commands/transfer";
import { clearTransferHistory } from "@/commands/transfer";
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
import { cn } from "@/lib/utils";

export const Route = createLazyFileRoute("/_app/transfer/")({
  component: TransferPage,
});

function TransferPage() {
  const breakpoint = useBreakpoint();
  const isMobile = breakpoint === "mobile";

  const sessions = useTransferStore((s) => s.sessions);
  const dbHistory = useTransferStore((s) => s.dbHistory);
  const loadHistory = useTransferStore((s) => s.loadHistory);

  // 进入传输列表页时主动刷新 DB 历史
  useEffect(() => {
    loadHistory();
  }, [loadHistory]);

  const [statusFilter, setStatusFilter] = useState<
    HistorySessionStatus | "all"
  >("all");

  const statusFilters: { value: HistorySessionStatus | "all"; label: string }[] = useMemo(
    () => [
      { value: "all", label: t`全部` },
      { value: "completed", label: t`已完成` },
      { value: "failed", label: t`失败` },
      { value: "paused", label: t`已暂停` },
      { value: "cancelled", label: t`已取消` },
    ],
    [],
  );

  // 活跃传输 sessionId 列表（按开始时间倒序）
  const activeSessionIds = useMemo(
    () =>
      Object.values(sessions)
        .sort((a, b) => b.startedAt - a.startedAt)
        .map((s) => s.sessionId),
    [sessions],
  );

  // 过滤 DB 历史
  const filteredHistory = useMemo(
    () =>
      statusFilter === "all"
        ? dbHistory
        : dbHistory.filter((item) => item.status === statusFilter),
    [dbHistory, statusFilter],
  );

  const hasContent = activeSessionIds.length > 0 || filteredHistory.length > 0;

  const handleClearHistory = async () => {
    try {
      await clearTransferHistory();
      await loadHistory();
      toast.success(t`已清空传输历史`);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };

  // 「传输历史」section 标题右侧的过滤 + 清空操作
  const historyToolbar = dbHistory.length > 0 && (
    <div className="flex items-center gap-1.5 md:gap-2">
      {/* 状态过滤 */}
      <Select
        value={statusFilter}
        onValueChange={(v) => setStatusFilter(v as HistorySessionStatus | "all")}
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
      activeSessionIds={activeSessionIds}
      historyItems={filteredHistory}
      historyToolbar={historyToolbar}
    />
  );

  return (
    <main className="flex h-full flex-1 flex-col bg-background">
      {/* Page Content —— 页面标题由 AppTopBar 面包屑承担,无独立 header */}
      <div className={cn(
        "flex-1 overflow-auto",
        isMobile ? "px-3 py-3" : "p-5 lg:p-6",
      )}>
        {content}
      </div>
    </main>
  );
}

/* ─────────────────── 传输列表 ─────────────────── */

function TransferList({
  activeSessionIds,
  historyItems,
  historyToolbar,
}: {
  activeSessionIds: string[];
  historyItems: TransferHistoryItem[];
  historyToolbar: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-5">
      {/* 活跃传输 */}
      {activeSessionIds.length > 0 && (
        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-foreground">
            <Trans>活跃传输</Trans>
          </h2>
          <div className="flex flex-col gap-2.5">
            {activeSessionIds.map((id) => (
              <TransferItem key={id} sessionId={id} />
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
