/**
 * Transfer Page (Lazy)
 * 活动中心 —— 所有传输会话（进行中 / 可恢复 / 已结束）的 master-detail 单页。
 *
 * 响应式（与收件箱共用 MasterDetailShell，单一标准）：
 * - 宽屏（≥920px）：左「会话列表」+ 右「会话详情」双栏。
 * - 窄屏（<920px）：详情占满，列表从左侧抽屉滑出。
 * 有内容时自动选中首项（详情区默认有内容），仅一条都没有才显示空态。
 * 选中态由 search param `?session=` 承载，过滤器由 `?filter=` 承载；
 * 旧 /transfer/$sessionId 深链重定向至此。
 */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowRightLeft, Trash2 } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import { commands, type TransferProjection } from "@/lib/bindings";
import { useSessionProgress, useTransferStore } from "@/stores/transfer-store";
import {
  canResumeProjection,
  isProjectionActive,
  isProjectionEnded,
} from "@/lib/transfer-projection";
import { getErrorMessage } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { CenteredEmptyState } from "@/components/layout/section-primitives";
import {
  MasterDetailShell,
  OpenListButton,
} from "@/components/layout/master-detail-shell";
import {
  SessionActions,
  SessionFileSection,
  SessionProgressBlock,
  SessionSummaryHeader,
} from "@/components/transfer/session-panel";
import { DESTRUCTIVE_BTN_CLASS, SessionRow } from "./-session-row";
import type { TransferFilterKey } from "./index";

export const Route = createLazyFileRoute("/_app/transfer/")({
  component: TransferPage,
});

/* ─────────────────── 过滤器 ─────────────────── */

type FilterKey = TransferFilterKey;

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
  const { session: selectedId, filter: routeFilter } = Route.useSearch();
  const projections = useTransferStore((s) => s.projections);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  const filter: FilterKey = routeFilter ?? "all";
  const [clearOpen, setClearOpen] = useState(false);

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

  const selectSession = useCallback(
    (sessionId: string | null) => {
      void navigate({
        to: "/transfer",
        search: {
          ...(sessionId ? { session: sessionId } : {}),
          ...(filter !== "all" ? { filter } : {}),
        },
        replace: true,
      });
    },
    [filter, navigate],
  );

  const selectFilter = useCallback(
    (nextFilter: FilterKey) => {
      void navigate({
        to: "/transfer",
        search: {
          ...(selectedId ? { session: selectedId } : {}),
          ...(nextFilter !== "all" ? { filter: nextFilter } : {}),
        },
        replace: true,
      });
    },
    [navigate, selectedId],
  );

  // 自动选首项：无有效选中（未选 / 选中项已删除）且有内容 → 选第一条；零内容 → 清空走空态。
  // 不在筛选切换时重选：用户已选的会话即使被当前筛选隐藏也保留，避免选中态跳动。
  useEffect(() => {
    if (items.length === 0) {
      if (selectedId) selectSession(null);
    } else if (!selectedId || !projections[selectedId]) {
      selectSession(items[0].sessionId);
    }
  }, [items, selectedId, projections, selectSession]);

  // 详情展示项：选中优先，否则回落首项（避免自动选中 URL 更新前的空窗闪烁）。
  const shown =
    (selectedId ? projections[selectedId] : undefined) ?? items[0] ?? null;

  const handleClearConfirm = async () => {
    setClearOpen(false);
    try {
      await commands.clearTransferHistory();
      await loadProjections();
      toast.success(t`已清空活动记录`);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };

  return (
    <>
      <MasterDetailShell
        drawerLabel={t`传输活动`}
        listMaxWidth={370}
        list={({ closeDrawer }) => (
          <SessionRail
            items={visibleItems}
            totalCount={items.length}
            counts={counts}
            filter={filter}
            onFilterChange={selectFilter}
            selectedId={shown?.sessionId ?? null}
            onSelect={selectSession}
            onAfterSelect={closeDrawer}
            onClear={items.length > 0 ? () => setClearOpen(true) : null}
          />
        )}
        detail={({ openList, isCompact }) =>
          shown ? (
            <SessionDetail
              projection={shown}
              onSessionChange={selectSession}
              openList={openList}
              isCompact={isCompact}
            />
          ) : (
            <TransferDetailEmpty openList={openList} isCompact={isCompact} />
          )
        }
      />

      {clearOpen && (
        <ConfirmDialog
          open
          onOpenChange={setClearOpen}
          title={<Trans>清空全部传输记录？</Trans>}
          description={
            <Trans>
              将删除全部 {counts.all} 条记录，包括可恢复任务的断点信息，此操作无法撤销；已传输的文件不受影响。
            </Trans>
          }
          confirmLabel={<Trans>清空记录</Trans>}
          onConfirm={handleClearConfirm}
        />
      )}
    </>
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
  onAfterSelect,
  onClear,
}: {
  items: TransferProjection[];
  totalCount: number;
  counts: Record<FilterKey, number>;
  filter: FilterKey;
  onFilterChange: (filter: FilterKey) => void;
  selectedId: string | null;
  onSelect: (sessionId: string) => void;
  /** 选中后关抽屉（窄屏）。与 onSelect 均为稳定引用，合成的 handleSelect 才能保住 SessionRow memo。 */
  onAfterSelect: () => void;
  onClear: (() => void) | null;
}) {
  // 合成稳定回调下发给 memo 化的 SessionRow：onSelect / onAfterSelect 均稳定 → 引用不变。
  const handleSelect = useCallback(
    (id: string) => {
      onSelect(id);
      onAfterSelect();
    },
    [onSelect, onAfterSelect],
  );

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
          <p className="px-2 py-8 text-center text-xs text-muted-foreground">
            <Trans>暂无传输活动</Trans>
          </p>
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
              onSelect={handleSelect}
              onSessionChange={handleSelect}
            />
          ))
        )}
      </div>
    </div>
  );
}

function DetailEmptyState() {
  return (
    <CenteredEmptyState
      icon={ArrowRightLeft}
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

/* ─────────────────── 右栏 / 窄屏主区：会话详情 ─────────────────── */

/** 详情面板外壳：自包裹 glass-panel，宽屏内部滚动、窄屏随页面滚动（isCompact）。 */
function DetailShell({
  openList,
  isCompact,
  children,
}: {
  openList: (() => void) | null;
  isCompact: boolean;
  children: ReactNode;
}) {
  return (
    <section
      className={cn(
        "glass-panel flex flex-col rounded-[24px]",
        isCompact ? "flex-1" : "min-h-0 flex-1 overflow-hidden",
      )}
    >
      {isCompact && openList && (
        <div className="flex h-12 shrink-0 items-center px-4">
          <OpenListButton openList={openList} label={t`会话列表`} />
        </div>
      )}
      {children}
    </section>
  );
}

function SessionDetail({
  projection,
  onSessionChange,
  openList,
  isCompact,
}: {
  projection: TransferProjection;
  onSessionChange: (sessionId: string) => void;
  openList: (() => void) | null;
  isCompact: boolean;
}) {
  const progress = useSessionProgress(projection.sessionId);

  return (
    <DetailShell openList={openList} isCompact={isCompact}>
      <div
        className={cn(
          "flex flex-col gap-5 p-4 lg:p-5",
          isCompact ? "flex-1" : "min-h-0 flex-1",
        )}
      >
        <SessionSummaryHeader projection={projection} />
        <SessionProgressBlock projection={projection} progress={progress} />
        <SessionFileSection
          projection={projection}
          progress={progress}
          className="min-h-0 flex-1"
        />
        <SessionActions
          projection={projection}
          onSessionChange={onSessionChange}
        />
      </div>
    </DetailShell>
  );
}

/** 零会话时的详情空态；列表侧同时显示精简的「暂无传输活动」提示。 */
function TransferDetailEmpty({
  openList,
  isCompact,
}: {
  openList: (() => void) | null;
  isCompact: boolean;
}) {
  return (
    <DetailShell openList={openList} isCompact={isCompact}>
      <div className={cn("flex flex-col", isCompact ? "flex-1" : "min-h-0 flex-1")}>
        <DetailEmptyState />
      </div>
    </DetailShell>
  );
}
