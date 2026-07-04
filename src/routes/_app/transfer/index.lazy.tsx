/**
 * Transfer Page (Lazy)
 * 活动中心 —— 所有传输会话（进行中 / 可恢复 / 已结束）的 master-detail 单页。
 *
 * 响应式：
 * - 桌面（≥1024px）：左「会话列表」+ 右「会话详情」两栏并排，各自内部滚动。
 * - 窄屏（<1024px）：列表占满整宽；选中会话后详情从右侧抽屉滑出。
 * 选中态由 search param `?session=` 承载，旧 /transfer/$sessionId 深链重定向至此。
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowLeftRight, MousePointerClick, Trash2, X } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import { commands, type TransferProjection } from "@/lib/bindings";
import { useSessionProgress, useTransferStore } from "@/stores/transfer-store";
import { useMediaQuery } from "@/hooks/use-media-query";
import {
  canResumeProjection,
  isProjectionActive,
  isProjectionEnded,
} from "@/lib/transfer-projection";
import { getErrorMessage } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { CenteredEmptyState } from "@/components/layout/section-primitives";
import {
  SessionActions,
  SessionFileSection,
  SessionProgressBlock,
  SessionSummaryHeader,
} from "@/components/transfer/session-panel";
import { DESTRUCTIVE_BTN_CLASS, SessionRow } from "./-session-row";

export const Route = createLazyFileRoute("/_app/transfer/")({
  component: TransferPage,
});

/* ─────────────────── 过滤器 ─────────────────── */

type FilterKey = "all" | "active" | "recoverable" | "ended";

function matchesFilter(item: TransferProjection, filter: FilterKey): boolean {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return isProjectionActive(item);
    case "recoverable":
      return canResumeProjection(item);
    case "ended":
      return isProjectionEnded(item);
  }
}

/** 列表排序权重：进行中 > 可恢复/中断 > 已结束，组内按开始时间降序 */
function phaseRank(item: TransferProjection): number {
  if (isProjectionActive(item)) return 0;
  if (item.phase === "suspended") return 1;
  return 2;
}

/* ─────────────────── 页面 ─────────────────── */

function TransferPage() {
  const navigate = useNavigate();
  const { session: selectedId } = Route.useSearch();
  const projections = useTransferStore((s) => s.projections);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  const isDesktop = useMediaQuery("(min-width: 1024px)");
  const [filter, setFilter] = useState<FilterKey>("all");

  // 进入页面时主动刷新后端 projection（删除路径的权威来源）
  useEffect(() => {
    loadProjections();
  }, [loadProjections]);

  const items = useMemo(
    () =>
      Object.values(projections).sort(
        (a, b) => phaseRank(a) - phaseRank(b) || b.startedAt - a.startedAt,
      ),
    [projections],
  );

  const counts = useMemo(() => {
    const acc: Record<FilterKey, number> = {
      all: items.length,
      active: 0,
      recoverable: 0,
      ended: 0,
    };
    for (const item of items) {
      if (isProjectionActive(item)) acc.active++;
      else if (canResumeProjection(item)) acc.recoverable++;
      else if (isProjectionEnded(item)) acc.ended++;
    }
    return acc;
  }, [items]);

  const visibleItems = useMemo(
    () => items.filter((i) => matchesFilter(i, filter)),
    [items, filter],
  );

  const selected = selectedId ? (projections[selectedId] ?? null) : null;

  const selectSession = useCallback(
    (sessionId: string | null) => {
      void navigate({
        to: "/transfer",
        search: sessionId ? { session: sessionId } : {},
        replace: true,
      });
    },
    [navigate],
  );

  // 选中会话被删除 / 清空后自动清掉选中态
  useEffect(() => {
    if (selectedId && !projections[selectedId]) {
      selectSession(null);
    }
  }, [selectedId, projections, selectSession]);

  const handleClearHistory = async () => {
    try {
      await commands.clearTransferHistory();
      await loadProjections();
      toast.success(t`已清空活动记录`);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };

  // 窄屏详情抽屉：Esc 关闭 + 焦点移入面板
  const drawerOpen = !isDesktop && selected !== null;
  const drawerPanelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!drawerOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") selectSession(null);
    };
    document.addEventListener("keydown", onKey);
    const raf = requestAnimationFrame(() => drawerPanelRef.current?.focus());
    return () => {
      document.removeEventListener("keydown", onKey);
      cancelAnimationFrame(raf);
    };
  }, [drawerOpen, selectSession]);

  const rail = (
    <SessionRail
      items={visibleItems}
      totalCount={items.length}
      counts={counts}
      filter={filter}
      onFilterChange={setFilter}
      selectedId={selectedId ?? null}
      onSelect={selectSession}
      onClear={items.length > 0 ? handleClearHistory : null}
    />
  );

  return (
    <main className="relative flex h-full flex-1 flex-col bg-transparent">
      {isDesktop ? (
        <div className="mx-auto grid h-full w-full max-w-[1240px] grid-cols-[minmax(300px,370px)_minmax(0,1fr)] grid-rows-1 gap-6 overflow-hidden p-6">
          <section className="glass-panel flex min-h-0 flex-col overflow-hidden rounded-[24px]">
            {rail}
          </section>
          <section className="glass-panel flex min-h-0 flex-col overflow-hidden rounded-[24px]">
            {selected ? (
              <SessionDetail
                projection={selected}
                onSessionChange={selectSession}
              />
            ) : (
              <DetailPlaceholder hasItems={items.length > 0} />
            )}
          </section>
        </div>
      ) : (
        <>
          <div className="mx-auto flex h-full w-full max-w-[880px] flex-col overflow-hidden p-4 sm:p-5">
            <section className="glass-panel flex min-h-0 flex-1 flex-col overflow-hidden rounded-[24px]">
              {rail}
            </section>
          </div>

          {/* 详情抽屉：限定在顶栏下方的内容区滑出，遮罩只暗内容、不盖全局顶栏 */}
          <div
            className={cn(
              "absolute inset-0 z-30",
              !drawerOpen && "pointer-events-none",
            )}
          >
            <div
              onClick={() => selectSession(null)}
              className={cn(
                "absolute inset-0 bg-black/40 transition-opacity duration-300 motion-reduce:transition-none",
                drawerOpen ? "opacity-100" : "opacity-0",
              )}
            />
            <div
              ref={drawerPanelRef}
              role="dialog"
              aria-modal="true"
              aria-label={t`传输详情`}
              tabIndex={-1}
              inert={!drawerOpen}
              className={cn(
                "absolute inset-y-0 right-0 flex w-[92%] max-w-[430px] flex-col rounded-l-[24px] border-l border-[color:var(--glass-control-border)] bg-background shadow-2xl outline-hidden transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none",
                drawerOpen ? "translate-x-0" : "translate-x-full",
              )}
            >
              <div className="flex items-center justify-between px-4 pt-4">
                <h2 className="text-sm font-semibold text-foreground">
                  <Trans>传输详情</Trans>
                </h2>
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-7 text-muted-foreground"
                  onClick={() => selectSession(null)}
                  title={t`关闭`}
                >
                  <X className="size-4" />
                </Button>
              </div>
              {selected && (
                <SessionDetail
                  projection={selected}
                  onSessionChange={selectSession}
                />
              )}
            </div>
          </div>
        </>
      )}
    </main>
  );
}

/* ─────────────────── 左栏：会话列表 ─────────────────── */

function SessionRail({
  items,
  totalCount,
  counts,
  filter,
  onFilterChange,
  selectedId,
  onSelect,
  onClear,
}: {
  items: TransferProjection[];
  totalCount: number;
  counts: Record<FilterKey, number>;
  filter: FilterKey;
  onFilterChange: (filter: FilterKey) => void;
  selectedId: string | null;
  onSelect: (sessionId: string) => void;
  onClear: (() => Promise<void>) | null;
}) {
  const filters: { key: FilterKey; label: ReactNode }[] = [
    { key: "all", label: <Trans>全部</Trans> },
    { key: "active", label: <Trans>进行中</Trans> },
    { key: "recoverable", label: <Trans>可恢复</Trans> },
    { key: "ended", label: <Trans>已结束</Trans> },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex items-center justify-between gap-3 px-4 pb-2 pt-4">
        <h1 className="text-[15px] font-semibold tracking-tight text-foreground">
          <Trans>传输活动</Trans>
        </h1>
        {onClear && (
          <Button
            size="icon"
            variant="ghost"
            className={DESTRUCTIVE_BTN_CLASS}
            onClick={onClear}
            title={t`清空活动记录`}
          >
            <Trash2 className="size-3.5" />
          </Button>
        )}
      </header>

      <div className="flex flex-wrap items-center gap-1.5 px-4 pb-3">
        {filters.map(({ key, label }) => (
          <button
            key={key}
            type="button"
            onClick={() => onFilterChange(key)}
            aria-pressed={filter === key}
            className={cn(
              "flex items-center gap-1 rounded-full px-2.5 py-1 text-[11px] font-medium transition-colors",
              filter === key
                ? "bg-primary/15 text-brand"
                : "text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground dark:hover:bg-white/[0.07]",
            )}
          >
            {label}
            <span className="font-mono tabular-nums">{counts[key]}</span>
          </button>
        ))}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-3 pt-0">
        {totalCount === 0 ? (
          <RailEmptyState />
        ) : items.length === 0 ? (
          <p className="px-2 py-8 text-center text-xs text-muted-foreground">
            <Trans>此分类下暂无记录</Trans>
          </p>
        ) : (
          items.map((item) => (
            <SessionRow
              key={item.sessionId}
              projection={item}
              selected={item.sessionId === selectedId}
              onSelect={onSelect}
              onSessionChange={onSelect}
            />
          ))
        )}
      </div>
    </div>
  );
}

function RailEmptyState() {
  return (
    <CenteredEmptyState
      icon={ArrowLeftRight}
      title={<Trans>暂无传输活动</Trans>}
      description={
        <Trans>
          在文件管理器中右键「用 SwarmDrop
          发送」，或在设备页选中目标设备发起传输；成功接收的文件会自动进入收件箱。
        </Trans>
      }
      descriptionClassName="max-w-[30ch]"
    />
  );
}

/* ─────────────────── 右栏 / 抽屉：会话详情 ─────────────────── */

function SessionDetail({
  projection,
  onSessionChange,
}: {
  projection: TransferProjection;
  onSessionChange: (sessionId: string) => void;
}) {
  const progress = useSessionProgress(projection.sessionId);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5 p-4 lg:p-5">
      <SessionSummaryHeader projection={projection} />
      <SessionProgressBlock projection={projection} progress={progress} />
      <SessionFileSection
        projection={projection}
        progress={progress}
        className="min-h-0 flex-1"
      />
      <SessionActions projection={projection} onSessionChange={onSessionChange} />
    </div>
  );
}

function DetailPlaceholder({ hasItems }: { hasItems: boolean }) {
  return (
    <CenteredEmptyState
      icon={MousePointerClick}
      title={<Trans>选择一个会话</Trans>}
      description={
        hasItems ? (
          <Trans>从左侧列表选择会话，查看进度、文件明细和操作。</Trans>
        ) : (
          <Trans>发起或接收传输后，会话会出现在左侧列表。</Trans>
        )
      }
    />
  );
}
