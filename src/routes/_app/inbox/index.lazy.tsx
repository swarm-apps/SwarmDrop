/**
 * Drop Inbox Page (Lazy)
 * 收件箱 —— 展示已经成功接收的内容，和活动/恢复过程账本分离。
 *
 * 响应式：
 * - 桌面（≥1024px）：左「时间分组导航栏」+ 右「阅读区」两栏并排，各自内部滚动。
 * - 窄屏（<1024px）：阅读区占满整宽 + 整页滚动；列表收成左侧抽屉（Sheet），
 *   顶部「收件箱」按钮唤出，选中即收起，避免上下堆叠 / 双滚动。
 * 「来源与过程」用容器查询按阅读区自身宽度决定单列/双列。
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import {
  Archive,
  ArchiveRestore,
  ArrowRightLeft,
  Bot,
  FileArchive,
  FolderOpen,
  Inbox,
  PanelLeft,
  RefreshCw,
  Search,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { Plural, Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import {
  commands,
  type InboxItemDetail,
  type InboxItemFileEntry,
  type InboxItemSummary,
  type InboxSearchHit,
  type TransferProjection,
} from "@/lib/bindings";
import { getFileIcon, getFileIconColor } from "@/lib/file-icon";
import { useInboxStore } from "@/stores/inbox-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { CenteredEmptyState } from "@/components/layout/section-primitives";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { cn } from "@/lib/utils";
import { formatFileSize, formatRelativeTime } from "@/lib/format";
import { projectionStatusLabel } from "@/lib/transfer-projection";
import { MasterDetailShell } from "@/components/layout/master-detail-shell";

export const Route = createLazyFileRoute("/_app/inbox/")({
  component: InboxPage,
});

type ReaderStatus = "empty" | "loading" | "error" | "ready";

function InboxPage() {
  const detail = useInboxStore((s) => s.detail);
  const detailForId = useInboxStore((s) => s.detailForId);
  const selectedId = useInboxStore((s) => s.selectedId);
  const items = useInboxStore((s) => s.items);
  const query = useInboxStore((s) => s.query);
  const searchResults = useInboxStore((s) => s.searchResults);
  const showArchived = useInboxStore((s) => s.showArchived);
  const loadItems = useInboxStore((s) => s.loadItems);
  const loadDetail = useInboxStore((s) => s.loadDetail);
  const runAndRefresh = useInboxStore((s) => s.runAndRefresh);
  const runSearch = useInboxStore((s) => s.runSearch);

  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleteLocalFiles, setDeleteLocalFiles] = useState(false);

  const navigate = useNavigate();
  const selectItem = useInboxStore((s) => s.selectItem);
  const isSearching = query.trim() !== "";
  const selectedItem = useMemo(
    () => items.find((item) => item.id === selectedId) ?? null,
    [items, selectedId],
  );
  const visibleIds = useMemo(
    () =>
      isSearching
        ? (searchResults ?? []).map((h) => h.id)
        : items.map((i) => i.id),
    [isSearching, searchResults, items],
  );
  const selectionVisible =
    selectedId !== null && (!isSearching || visibleIds.includes(selectedId));

  // keep-previous：切换条目时保留上一份详情直到新详情到达，避免旧 detail.id ≠ selectedId
  // 的那一帧掉到骨架屏造成闪烁。骨架屏只在首次、完全无详情时出现。
  const readerStatus: ReaderStatus = !selectionVisible
    ? "empty"
    : detail
      ? "ready"
      : detailForId === selectedId
        ? "error"
        : "loading";

  // 进入页面 / 切换归档过滤时重新加载列表
  useEffect(() => {
    void loadItems();
  }, [loadItems, showArchived]);

  // 选中项变化时加载详情
  useEffect(() => {
    void loadDetail(selectedId);
  }, [loadDetail, selectedId]);

  // 搜索词 / 归档过滤变化 → 防抖触发检索
  useEffect(() => {
    if (query.trim() === "") return;
    const id = setTimeout(() => void runSearch(), 250);
    return () => clearTimeout(id);
  }, [query, showArchived, runSearch]);

  // 自动选首项：有内容且无有效选中（未选 / 选中项已不在可见列表）→ 选首个可见项；
  // 零内容不选（走空态）。搜索态下 visibleIds = 搜索结果，即自动选中首个命中。
  useEffect(() => {
    if (visibleIds.length === 0) return;
    if (selectedId === null || !visibleIds.includes(selectedId)) {
      selectItem(visibleIds[0]);
    }
  }, [visibleIds, selectedId, selectItem]);

  const handleReveal = () => {
    if (!selectedId) return;
    void runAndRefresh(() => commands.showInboxItemInFolder(selectedId, null));
  };
  const handleOpenTransfer = () => {
    if (!selectedItem?.transferSessionId) return;
    void navigate({
      to: "/transfer",
      search: { session: selectedItem.transferSessionId },
    });
  };
  const handleArchive = () => {
    if (!selectedItem) return;
    const archived = !selectedItem.archivedAt;
    void runAndRefresh(
      () => commands.archiveInboxItem(selectedItem.id, archived),
      archived ? t`已归档收件箱记录` : t`已取消归档`,
    );
  };
  const handleDeleteConfirm = async () => {
    if (!selectedId) return;
    await runAndRefresh(
      () => commands.deleteInboxItem(selectedId, deleteLocalFiles),
      deleteLocalFiles ? t`已删除记录和本地文件` : t`已删除收件箱记录`,
    );
    setDeleteLocalFiles(false);
    setDeleteOpen(false);
  };

  return (
    <>
      <MasterDetailShell
        drawerLabel={t`收件箱`}
        listMaxWidth={360}
        list={({ closeDrawer }) => <InboxRail onAfterSelect={closeDrawer} />}
        detail={({ openList, isCompact }) => (
          <InboxReader
            status={readerStatus}
            detail={detail}
            contained={!isCompact}
            onOpenList={openList ?? undefined}
            onReveal={handleReveal}
            onOpenTransfer={
              selectedItem?.transferSessionId ? handleOpenTransfer : null
            }
            onArchive={handleArchive}
            onDelete={() => setDeleteOpen(true)}
            onFileOpen={(fileId) =>
              detail &&
              runAndRefresh(() => commands.openInboxItem(detail.id, fileId))
            }
            onFileReveal={(fileId) =>
              detail &&
              runAndRefresh(() =>
                commands.showInboxItemInFolder(detail.id, fileId),
              )
            }
          />
        )}
      />

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>删除收件箱记录</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              <Trans>
                仅删除记录会保留本地文件；勾选删除本地文件后，会从磁盘移除这些已接收文件。
              </Trans>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="flex items-center justify-between rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2">
            <Label htmlFor="delete-local-files" className="text-sm text-foreground">
              <Trans>同时删除本地文件</Trans>
            </Label>
            <Switch
              id="delete-local-files"
              checked={deleteLocalFiles}
              onCheckedChange={setDeleteLocalFiles}
            />
          </div>
          {deleteLocalFiles && (
            <p className="flex items-start gap-2 text-xs leading-5 text-destructive">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
              <Trans>本地文件删除后无法通过清空活动记录恢复。</Trans>
            </p>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel>
              <Trans>取消</Trans>
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteConfirm}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {deleteLocalFiles ? (
                <Trans>删除记录和文件</Trans>
              ) : (
                <Trans>仅删除记录</Trans>
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/* ─────────────────── 左栏 / 抽屉：时间分组导航 ─────────────────── */

function InboxRail({ onAfterSelect }: { onAfterSelect?: () => void }) {
  const items = useInboxStore((s) => s.items);
  const selectedId = useInboxStore((s) => s.selectedId);
  const loading = useInboxStore((s) => s.loading);
  const showArchived = useInboxStore((s) => s.showArchived);
  const setShowArchived = useInboxStore((s) => s.setShowArchived);
  const selectItem = useInboxStore((s) => s.selectItem);
  const runAndRefresh = useInboxStore((s) => s.runAndRefresh);
  const query = useInboxStore((s) => s.query);
  const searching = useInboxStore((s) => s.searching);
  const searchResults = useInboxStore((s) => s.searchResults);
  const setQuery = useInboxStore((s) => s.setQuery);

  const isSearching = query.trim() !== "";
  const groups = useMemo(() => groupInboxByTime(items), [items]);
  const railScrollRef = useRef<HTMLDivElement>(null);

  const visibleIds = useMemo(
    () =>
      isSearching
        ? (searchResults ?? []).map((h) => h.id)
        : items.map((i) => i.id),
    [isSearching, searchResults, items],
  );

  const handleSelect = (id: string) => {
    selectItem(id);
    onAfterSelect?.();
  };

  const moveSelection = useCallback(
    (delta: number) => {
      if (visibleIds.length === 0) return;
      const idx = selectedId ? visibleIds.indexOf(selectedId) : -1;
      const nextIdx =
        idx < 0
          ? delta > 0
            ? 0
            : visibleIds.length - 1
          : Math.min(visibleIds.length - 1, Math.max(0, idx + delta));
      const nextId = visibleIds[nextIdx];
      if (!nextId || nextId === selectedId) return;
      selectItem(nextId);
      requestAnimationFrame(() => {
        railScrollRef.current
          ?.querySelector<HTMLElement>(`[data-inbox-id="${CSS.escape(nextId)}"]`)
          ?.focus();
      });
    },
    [visibleIds, selectedId, selectItem],
  );

  const handleRailKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveSelection(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveSelection(-1);
    }
  };

  const handleRepair = () =>
    runAndRefresh(async () => {
      const repaired = await commands.repairMissingInboxItems();
      toast.success(t`已检查收件箱`, {
        description:
          repaired.length > 0
            ? t`补建了 ${repaired.length} 条收件箱记录`
            : t`没有缺失的收件箱记录`,
      });
    });

  return (
    <div className="flex h-full flex-col p-3">
      <div className="shrink-0 px-1.5 pt-1">
        <div className="flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2.5">
            <span className="glass-control flex size-8 shrink-0 items-center justify-center rounded-full text-brand">
              <Inbox className="size-3.5" />
            </span>
            <h2 className="text-[15px] font-semibold tracking-tight text-foreground">
              <Trans>收件箱</Trans>
            </h2>
            {!isSearching && items.length > 0 && (
              <span className="rounded-full bg-foreground/[0.045] px-2 py-0.5 text-[11px] font-semibold tabular-nums text-muted-foreground dark:bg-white/[0.06]">
                {items.length}
              </span>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-8 text-muted-foreground hover:text-foreground"
              title={t`补建缺失记录`}
              onClick={handleRepair}
            >
              <RefreshCw className="size-4" />
            </Button>
            <Button
              type="button"
              variant={showArchived ? "secondary" : "ghost"}
              size="sm"
              className="h-8 gap-1.5 px-2.5 text-xs"
              onClick={() => setShowArchived(!showArchived)}
            >
              <Archive className="size-3.5" />
              <Trans>归档</Trans>
            </Button>
          </div>
        </div>

        <div className="relative mt-4">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label={t`搜索收件箱`}
            placeholder={t`搜索标题、来源、文件名…`}
            className="h-9 rounded-[14px] border-transparent bg-foreground/[0.045] pl-9 text-sm dark:bg-white/[0.05]"
          />
        </div>
      </div>

      <div
        ref={railScrollRef}
        onKeyDown={handleRailKeyDown}
        className="mt-3 min-h-0 flex-1 scroll-pt-10 overflow-auto px-1 pb-1"
      >
        {isSearching ? (
          searching ? (
            <ListSkeleton />
          ) : searchResults && searchResults.length > 0 ? (
            <div className="flex flex-col gap-1.5">
              {searchResults.map((hit) => (
                <SearchRow
                  key={hit.id}
                  hit={hit}
                  query={query}
                  selected={hit.id === selectedId}
                  onSelect={handleSelect}
                />
              ))}
            </div>
          ) : (
            <SearchEmptyState />
          )
        ) : loading ? (
          <ListSkeleton />
        ) : items.length === 0 ? (
          <InboxEmptyState />
        ) : (
          <div className="flex flex-col gap-5">
            {groups.map((group) => (
              <RailGroup
                key={group.key}
                group={group}
                selectedId={selectedId}
                onSelect={handleSelect}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function RailGroup({
  group,
  selectedId,
  onSelect,
}: {
  group: InboxTimeGroup;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <section className="flex flex-col gap-1.5">
      <div className="inbox-sticky-heading sticky top-0 z-10 flex items-center gap-2 rounded-lg px-3 py-1.5">
        <span className="text-[11px] font-medium text-muted-foreground">
          {groupLabel(group.key)}
        </span>
        <span className="text-[11px] tabular-nums text-muted-foreground">
          {group.items.length}
        </span>
      </div>
      <div className="flex flex-col gap-1.5">
        {group.items.map((item) => (
          <InboxRailRow
            key={item.id}
            item={item}
            selected={item.id === selectedId}
            onSelect={onSelect}
          />
        ))}
      </div>
    </section>
  );
}

function RailRowShell({
  id,
  iconTitle,
  iconCount,
  receivedAt,
  selected,
  onSelect,
  children,
}: {
  id: string;
  iconTitle: string;
  iconCount: number;
  receivedAt: number;
  selected: boolean;
  onSelect: (id: string) => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      data-inbox-id={id}
      aria-current={selected ? "true" : undefined}
      onClick={() => onSelect(id)}
      className={cn(
        "flex w-full scroll-mt-10 items-center gap-3 rounded-[14px] px-3 py-3 text-left outline-hidden transition-[background-color,box-shadow] duration-200 motion-reduce:transition-none",
        "focus-visible:ring-2 focus-visible:ring-ring",
        selected
          ? "bg-primary/10 ring-1 ring-primary/20"
          : "hover:bg-foreground/[0.045] dark:hover:bg-white/[0.05]",
      )}
    >
      <span className="glass-control flex size-10 shrink-0 items-center justify-center rounded-[14px] text-brand">
        <ItemIcon title={iconTitle} count={iconCount} />
      </span>
      <span className="min-w-0 flex-1">{children}</span>
      <span className="shrink-0 self-start pt-0.5 text-[11px] tabular-nums text-muted-foreground">
        {formatRelativeTime(receivedAt)}
      </span>
    </button>
  );
}

function InboxRailRow({
  item,
  selected,
  onSelect,
}: {
  item: InboxItemSummary;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <RailRowShell
      id={item.id}
      iconTitle={item.title}
      iconCount={item.itemCount}
      receivedAt={item.receivedAt}
      selected={selected}
      onSelect={onSelect}
    >
      <span className="flex min-w-0 items-center gap-1.5">
        <span className="truncate text-sm font-medium text-foreground">
          {item.title}
        </span>
        {item.sourceKind === "mcp" && (
          <Bot
            role="img"
            aria-label={t`AI 代理`}
            className="size-3.5 shrink-0 text-brand"
          />
        )}
        {item.missing && (
          <Pill tone="amber">
            <Trans>缺失</Trans>
          </Pill>
        )}
        {item.archivedAt && (
          <Pill tone="muted">
            <Trans>已归档</Trans>
          </Pill>
        )}
      </span>
      <span className="mt-1 block truncate text-xs text-muted-foreground">
        <Trans>来自 {item.sourceName}</Trans>
        <Dot />
        <Plural value={item.itemCount} other="# 项" />
        <Dot />
        {formatFileSize(item.totalSize)}
      </span>
    </RailRowShell>
  );
}

function SearchRow({
  hit,
  query,
  selected,
  onSelect,
}: {
  hit: InboxSearchHit;
  query: string;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <RailRowShell
      id={hit.id}
      iconTitle={hit.title}
      iconCount={hit.itemCount}
      receivedAt={hit.receivedAt}
      selected={selected}
      onSelect={onSelect}
    >
      <span className="block truncate text-sm font-medium text-foreground">
        {hit.title}
      </span>
      <span className="mt-1 block truncate text-xs text-muted-foreground">
        <Trans>来自 {hit.sourceName}</Trans>
        <Dot />
        <Plural value={hit.itemCount} other="# 项" />
      </span>
      {hit.snippet && (
        <span className="mt-1 block truncate text-xs text-muted-foreground">
          <HighlightedSnippet text={hit.snippet} query={query} />
        </span>
      )}
    </RailRowShell>
  );
}

/* ─────────────────── 右栏：阅读区 ─────────────────── */

function InboxReader({
  status,
  detail,
  contained,
  onOpenList,
  onReveal,
  onOpenTransfer,
  onArchive,
  onDelete,
  onFileOpen,
  onFileReveal,
}: {
  status: ReaderStatus;
  detail: InboxItemDetail | null;
  contained: boolean;
  /** 窄屏传入即渲染详情头部前导「打开列表」按钮；宽屏双栏不传。 */
  onOpenList?: () => void;
  onReveal: () => void;
  onOpenTransfer: (() => void) | null;
  onArchive: () => void;
  onDelete: () => void;
  onFileOpen: (fileId: number) => void;
  onFileReveal: (fileId: number) => void;
}) {
  const toggle = onOpenList ? (
    <Button
      variant="ghost"
      size="icon"
      className="-ml-1 size-8 shrink-0 rounded-md text-muted-foreground hover:text-foreground"
      onClick={onOpenList}
      aria-label={t`打开收件箱列表`}
      title={t`打开收件箱列表`}
    >
      <PanelLeft className="size-4" />
    </Button>
  ) : null;

  const sectionClass = cn(
    "glass-panel flex flex-col rounded-[24px]",
    contained && "min-h-0 overflow-hidden",
  );

  if (status === "ready" && detail) {
    return (
      <section className={sectionClass}>
        <ReaderContent
          detail={detail}
          contained={contained}
          leading={toggle}
          onReveal={onReveal}
          onOpenTransfer={onOpenTransfer}
          onArchive={onArchive}
          onDelete={onDelete}
          onFileOpen={onFileOpen}
          onFileReveal={onFileReveal}
        />
      </section>
    );
  }

  // empty / loading / error：前导按钮固定在同坐标的极简头行，正文在其下方
  return (
    <section className={sectionClass}>
      {toggle && (
        <div className="flex h-12 shrink-0 items-center border-b border-black/[0.06] px-7 dark:border-white/10">
          {toggle}
        </div>
      )}
      <div className={cn("flex flex-1 flex-col", !contained && "min-h-[320px]")}>
        {status === "empty" ? (
          <ReaderPlaceholder onOpenList={onOpenList} />
        ) : status === "loading" ? (
          <ReaderSkeleton />
        ) : (
          <ReaderErrorState />
        )}
      </div>
    </section>
  );
}

function ReaderContent({
  detail,
  contained,
  leading,
  onReveal,
  onOpenTransfer,
  onArchive,
  onDelete,
  onFileOpen,
  onFileReveal,
}: {
  detail: InboxItemDetail;
  contained: boolean;
  leading?: ReactNode;
  onReveal: () => void;
  /** 跳到该条目对应的传输记录；无关联传输时为 null（不渲染按钮）。 */
  onOpenTransfer: (() => void) | null;
  onArchive: () => void;
  onDelete: () => void;
  onFileOpen: (fileId: number) => void;
  onFileReveal: (fileId: number) => void;
}) {
  return (
    <>
      <header className="shrink-0 border-b border-black/[0.06] px-7 pb-5 pt-6 dark:border-white/10">
        <div className="flex items-start gap-3">
          {leading}
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-lg font-semibold tracking-tight text-foreground">
              {detail.title}
            </h2>
            <p className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-[13px] text-muted-foreground">
              <span>
                <Trans>来自 {detail.sourceName}</Trans>
              </span>
              <Dot />
              <Plural value={detail.itemCount} other="# 项" />
              <Dot />
              <span className="tabular-nums">
                {formatFileSize(detail.totalSize)}
              </span>
              <Dot />
              <span className="tabular-nums">
                {formatRelativeTime(detail.receivedAt)}
              </span>
            </p>
          </div>
          {detail.missing && (
            <span className="flex shrink-0 items-center gap-1 rounded-full bg-amber-500/15 px-2 py-1 text-xs font-medium text-amber-800 dark:text-amber-300">
              <TriangleAlert className="size-3.5" />
              <Trans>本地内容缺失</Trans>
            </span>
          )}
        </div>

        <div className="mt-5 flex flex-wrap gap-2">
          <Button size="sm" className="gap-1.5" onClick={onReveal}>
            <FolderOpen className="size-4" />
            <Trans>在文件夹中显示</Trans>
          </Button>
          {onOpenTransfer && (
            <Button
              size="sm"
              variant="outline"
              className="gap-1.5"
              onClick={onOpenTransfer}
            >
              <ArrowRightLeft className="size-4" />
              <Trans>打开传输记录</Trans>
            </Button>
          )}
          <Button size="sm" variant="ghost" className="gap-1.5" onClick={onArchive}>
            {detail.archivedAt ? (
              <ArchiveRestore className="size-4" />
            ) : (
              <Archive className="size-4" />
            )}
            {detail.archivedAt ? <Trans>取消归档</Trans> : <Trans>归档</Trans>}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="gap-1.5 text-destructive hover:bg-destructive/10 hover:text-destructive"
            onClick={onDelete}
          >
            <Trash2 className="size-4" />
            <Trans>删除</Trans>
          </Button>
        </div>
      </header>

      <div
        className={cn(
          "@container px-7 py-6",
          contained && "min-h-0 flex-1 overflow-auto",
        )}
      >
        <h3 className="text-sm font-semibold text-foreground">
          <Trans>文件</Trans>
        </h3>
        <div className="mt-4 grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(128px,1fr))]">
          {detail.files.map((file) => (
            <FileCard
              key={file.id}
              file={file}
              onOpen={() => onFileOpen(file.id)}
              onReveal={() => onFileReveal(file.id)}
            />
          ))}
        </div>

        <MetaSection item={detail} transfer={detail.transfer} />
      </div>
    </>
  );
}

function FileCard({
  file,
  onOpen,
  onReveal,
}: {
  file: InboxItemFileEntry;
  onOpen: () => void;
  onReveal: () => void;
}) {
  const [failed, setFailed] = useState(false);
  const src =
    isImageFile(file.name) && !failed ? thumbnailSrc(file.localPath) : null;
  const Icon = getFileIcon(file.name);

  return (
    <div className="group/card relative flex flex-col overflow-hidden rounded-[14px] border border-[color:var(--glass-control-border)] bg-foreground/[0.02] transition-colors hover:bg-foreground/[0.05] dark:bg-white/[0.03] dark:hover:bg-white/[0.06]">
      <button
        type="button"
        onClick={onOpen}
        title={t`打开 ${file.name}`}
        className="flex aspect-[4/3] w-full items-center justify-center overflow-hidden bg-muted/50 outline-hidden focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
      >
        {src ? (
          <img
            src={src}
            alt=""
            loading="lazy"
            decoding="async"
            onError={() => setFailed(true)}
            className="size-full object-cover"
          />
        ) : (
          <Icon className={cn("size-7", getFileIconColor(file.name))} />
        )}
      </button>

      <div className="flex items-start gap-1.5 px-2.5 py-2">
        <div className="min-w-0 flex-1">
          <p
            className="truncate text-[13px] font-medium text-foreground"
            title={file.relativePath}
          >
            {file.name}
          </p>
          <p className="mt-1 text-[11px] tabular-nums text-muted-foreground">
            {formatFileSize(file.size)}
          </p>
        </div>
        {file.missing && (
          <Pill tone="amber">
            <Trans>缺失</Trans>
          </Pill>
        )}
      </div>

      <div className="absolute right-2 top-2 opacity-0 transition-opacity group-focus-within/card:opacity-100 group-hover/card:opacity-100 motion-reduce:transition-none">
        <Button
          size="icon"
          variant="secondary"
          className="size-7 shadow-sm"
          title={t`在文件夹中显示`}
          onClick={onReveal}
        >
          <FolderOpen className="size-3.5" />
        </Button>
      </div>
    </div>
  );
}

function MetaSection({
  item,
  transfer,
}: {
  item: InboxItemSummary;
  transfer: TransferProjection | null;
}) {
  return (
    <section className="mt-9 border-t border-black/[0.06] pt-7 dark:border-white/10">
      <h3 className="text-sm font-semibold text-foreground">
        <Trans>来源与过程</Trans>
      </h3>
      <dl className="mt-5 grid grid-cols-1 gap-x-12 gap-y-5 @xl:grid-cols-2">
        <MetaRow label={t`来源设备`} value={item.sourceName} />
        <MetaRow label={t`来源类型`} value={sourceKindLabel(item.sourceKind)} />
        <MetaRow label={t`内容类型`} value={contentKindLabel(item.contentKind)} />
        <MetaRow label={t`接收时间`} value={new Date(item.receivedAt).toLocaleString()} />
        <MetaRow
          label={t`传输会话`}
          value={item.transferSessionId ?? t`已清理`}
          mono
        />
        <MetaRow label={t`本地位置`} value={item.rootPath ?? t`未知`} mono />
        {transfer && (
          <MetaRow label={t`活动状态`} value={projectionStatusLabel(transfer)} />
        )}
      </dl>
    </section>
  );
}

function MetaRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="min-w-0">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd
        className={cn(
          "mt-1.5 text-sm text-foreground",
          mono ? "break-all font-mono text-[12px]" : "break-words",
        )}
      >
        {value}
      </dd>
    </div>
  );
}

/* ─────────────────── 占位 / 骨架 / 空态 ─────────────────── */

function ReaderPlaceholder({ onOpenList }: { onOpenList?: () => void }) {
  return (
    <div className="flex h-full min-h-[280px] flex-col items-center justify-center gap-3 px-6 text-center">
      <div className="flex size-14 items-center justify-center rounded-full bg-muted">
        <Inbox className="size-7 text-muted-foreground" />
      </div>
      <p className="text-sm text-muted-foreground">
        <Trans>选择一条收件箱记录查看详情</Trans>
      </p>
      {onOpenList && (
        <Button
          variant="outline"
          size="sm"
          className="mt-1 gap-1.5"
          onClick={onOpenList}
        >
          <PanelLeft className="size-4" />
          <Trans>浏览收件箱</Trans>
        </Button>
      )}
    </div>
  );
}

function ReaderErrorState() {
  return (
    <CenteredEmptyState
      icon={TriangleAlert}
      title={<Trans>无法加载这条记录</Trans>}
      description={
        <Trans>记录可能已被移除，或详情加载失败。请选择其它记录，或刷新收件箱。</Trans>
      }
      descriptionClassName="max-w-[32ch]"
    />
  );
}

function ReaderSkeleton() {
  return (
    <div className="flex flex-col">
      <div className="border-b border-black/[0.06] px-7 pb-5 pt-6 dark:border-white/10">
        <Skeleton className="h-6 w-44" />
        <Skeleton className="mt-3 h-4 w-64" />
        <div className="mt-5 flex gap-2">
          <Skeleton className="h-8 w-20" />
          <Skeleton className="h-8 w-24" />
          <Skeleton className="h-8 w-20" />
        </div>
      </div>
      <div className="px-7 py-6">
        <Skeleton className="h-4 w-16" />
        <div className="mt-4 grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(128px,1fr))]">
          {SKELETON_KEYS.map((k) => (
            <div
              key={k}
              className="overflow-hidden rounded-[14px] border border-[color:var(--glass-control-border)]"
            >
              <Skeleton className="aspect-[4/3] w-full rounded-none" />
              <div className="space-y-2 px-3 py-2.5">
                <Skeleton className="h-3.5 w-3/4" />
                <Skeleton className="h-3 w-1/3" />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

const SKELETON_KEYS = ["s1", "s2", "s3", "s4", "s5", "s6"] as const;

function ListSkeleton() {
  return (
    <div className="flex flex-col gap-1.5 px-1">
      {SKELETON_KEYS.map((k) => (
        <div
          key={k}
          className="flex items-center gap-3 rounded-[14px] px-3 py-3"
        >
          <Skeleton className="size-10 rounded-[14px]" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-3.5 w-1/2" />
            <Skeleton className="h-3 w-3/4" />
          </div>
        </div>
      ))}
    </div>
  );
}

function InboxEmptyState() {
  return (
    <CenteredEmptyState
      icon={Inbox}
      title={<Trans>暂无已接收内容</Trans>}
      description={
        <Trans>成功接收的文件会出现在这里；暂停或失败的传输会留在活动与恢复。</Trans>
      }
      descriptionClassName="max-w-[26ch]"
    />
  );
}

function SearchEmptyState() {
  return (
    <CenteredEmptyState
      icon={Search}
      title={<Trans>未找到匹配项</Trans>}
      description={<Trans>试试更短的关键词，或检查是否包含已归档内容。</Trans>}
      descriptionClassName="max-w-[26ch]"
    />
  );
}

/* ─────────────────── 小构件 ─────────────────── */

function Dot() {
  return <span className="mx-1.5 text-foreground/25">·</span>;
}

function Pill({
  tone,
  children,
}: {
  tone: "amber" | "muted";
  children: ReactNode;
}) {
  return (
    <span
      className={cn(
        "shrink-0 rounded-full px-1.5 py-0.5 text-[11px] font-medium",
        tone === "amber" &&
          "bg-amber-500/15 text-amber-800 dark:text-amber-300",
        tone === "muted" &&
          "bg-foreground/[0.08] text-foreground/70 dark:bg-white/[0.08] dark:text-white/70",
      )}
    >
      {children}
    </span>
  );
}

function ItemIcon({ title, count }: { title: string; count: number }) {
  if (count > 1) return <FileArchive className="size-4.5 text-amber-500" />;
  const Icon = getFileIcon(title);
  return <Icon className={`size-4.5 ${getFileIconColor(title)}`} />;
}

/** 把 snippet 里匹配查询词的部分高亮（大小写不敏感）。 */
function HighlightedSnippet({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (!q) return <>{text}</>;
  const lower = text.toLowerCase();
  const needle = q.toLowerCase();
  const parts: ReactNode[] = [];
  let cursor = 0;
  let idx = lower.indexOf(needle);
  while (idx !== -1) {
    if (idx > cursor) parts.push(text.slice(cursor, idx));
    parts.push(
      <mark
        key={cursor}
        className="rounded-[3px] bg-primary/20 px-0.5 text-foreground"
      >
        {text.slice(idx, idx + q.length)}
      </mark>,
    );
    cursor = idx + q.length;
    idx = lower.indexOf(needle, cursor);
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return <>{parts}</>;
}

/* ─────────────────── 文件类型 / 缩略图 ─────────────────── */

const IMAGE_EXT = new Set([
  "jpg", "jpeg", "png", "gif", "webp", "bmp", "avif", "heic", "heif", "svg", "ico",
]);

function isImageFile(name: string): boolean {
  const i = name.lastIndexOf(".");
  return i >= 0 && IMAGE_EXT.has(name.slice(i + 1).toLowerCase());
}

/** 收件目录经 assetProtocol scope 暴露给 webview；失败回退 null（走类型图标）。 */
function thumbnailSrc(localPath: string): string | null {
  try {
    return convertFileSrc(localPath);
  } catch {
    return null;
  }
}

/* ─────────────────── 数据：时间分组 + 标签 ─────────────────── */

type InboxTimeGroupKey = "today" | "yesterday" | "week" | "earlier";

interface InboxTimeGroup {
  key: InboxTimeGroupKey;
  items: InboxItemSummary[];
}

/**
 * 按 receivedAt（毫秒时间戳）把条目分到 今天 / 昨天 / 本周 / 更早 四桶。
 * 边界用日历运算（new Date(y,m,d-n)）而非固定 24h，避开夏令时当日的 1h 偏移。
 * 后端已按接收时间倒序返回，桶内保持该顺序。
 */
function groupInboxByTime(items: InboxItemSummary[]): InboxTimeGroup[] {
  const now = new Date();
  const y = now.getFullYear();
  const m = now.getMonth();
  const d = now.getDate();
  const startOfToday = new Date(y, m, d).getTime();
  const startOfYesterday = new Date(y, m, d - 1).getTime();
  const startOfWeek = new Date(y, m, d - 6).getTime();

  const buckets: Record<InboxTimeGroupKey, InboxItemSummary[]> = {
    today: [],
    yesterday: [],
    week: [],
    earlier: [],
  };

  for (const item of items) {
    const ts = item.receivedAt;
    if (Number.isNaN(ts)) {
      buckets.earlier.push(item);
    } else if (ts >= startOfToday) {
      buckets.today.push(item);
    } else if (ts >= startOfYesterday) {
      buckets.yesterday.push(item);
    } else if (ts >= startOfWeek) {
      buckets.week.push(item);
    } else {
      buckets.earlier.push(item);
    }
  }

  const order: InboxTimeGroupKey[] = ["today", "yesterday", "week", "earlier"];
  return order
    .filter((k) => buckets[k].length > 0)
    .map((k) => ({ key: k, items: buckets[k] }));
}

function groupLabel(key: InboxTimeGroupKey) {
  switch (key) {
    case "today":
      return <Trans>今天</Trans>;
    case "yesterday":
      return <Trans>昨天</Trans>;
    case "week":
      return <Trans>本周</Trans>;
    case "earlier":
      return <Trans>更早</Trans>;
  }
}

function sourceKindLabel(kind: InboxItemSummary["sourceKind"]): string {
  const labels: Record<InboxItemSummary["sourceKind"], string> = {
    paired_device: t`已配对设备`,
    share_code: t`配对码`,
    mcp: t`AI 代理 (MCP)`,
    unknown: t`未知`,
  };
  return labels[kind];
}

function contentKindLabel(kind: InboxItemSummary["contentKind"]): string {
  const labels: Record<InboxItemSummary["contentKind"], string> = {
    files: t`文件`,
    text: t`文本`,
    clipboard: t`剪贴板`,
    bundle: t`Bundle`,
  };
  return labels[kind];
}
