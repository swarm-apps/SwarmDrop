/**
 * Drop Inbox Page (Lazy)
 * 收件箱 —— 展示已经成功接收的内容，和活动/恢复过程账本分离。
 */

import { useEffect, useMemo, useState, type ReactNode } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import {
  Archive,
  ArchiveRestore,
  Download,
  ExternalLink,
  FileArchive,
  FolderOpen,
  Inbox,
  Loader2,
  MapPin,
  RefreshCw,
  Search,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import {
  commands,
  type InboxItemDetail,
  type InboxItemSummary,
  type InboxSearchHit,
} from "@/lib/bindings";
import { getFileIcon, getFileIconColor } from "@/lib/file-icon";
import { useInboxStore } from "@/stores/inbox-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
import { pickFolder } from "@/lib/file-picker";
import { projectionStatusLabel } from "@/lib/transfer-projection";

export const Route = createLazyFileRoute("/_app/inbox/")({
  component: InboxPage,
});

function InboxPage() {
  const items = useInboxStore((s) => s.items);
  const selectedId = useInboxStore((s) => s.selectedId);
  const detail = useInboxStore((s) => s.detail);
  const loading = useInboxStore((s) => s.loading);
  const showArchived = useInboxStore((s) => s.showArchived);
  const loadItems = useInboxStore((s) => s.loadItems);
  const loadDetail = useInboxStore((s) => s.loadDetail);
  const selectItem = useInboxStore((s) => s.selectItem);
  const setShowArchived = useInboxStore((s) => s.setShowArchived);
  const runAndRefresh = useInboxStore((s) => s.runAndRefresh);
  const query = useInboxStore((s) => s.query);
  const searching = useInboxStore((s) => s.searching);
  const searchResults = useInboxStore((s) => s.searchResults);
  const setQuery = useInboxStore((s) => s.setQuery);
  const runSearch = useInboxStore((s) => s.runSearch);

  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleteLocalFiles, setDeleteLocalFiles] = useState(false);

  const selectedItem = useMemo(
    () => items.find((item) => item.id === selectedId) ?? null,
    [items, selectedId],
  );

  // 进入页面 / 切换归档过滤时重新加载列表
  useEffect(() => {
    void loadItems();
  }, [loadItems, showArchived]);

  // 选中项变化时加载详情
  useEffect(() => {
    void loadDetail(selectedId);
  }, [loadDetail, selectedId]);

  const isSearching = query.trim() !== "";

  // 搜索词 / 归档过滤变化 → 防抖触发检索（清空由 setQuery 即时退出搜索态）。
  useEffect(() => {
    if (query.trim() === "") return;
    const id = setTimeout(() => void runSearch(), 250);
    return () => clearTimeout(id);
  }, [query, showArchived, runSearch]);

  const handleRepair = () =>
    runAndRefresh(
      async () => {
        const repaired = await commands.repairMissingInboxItems();
        toast.success(t`已检查收件箱`, {
          description:
            repaired.length > 0
              ? t`补建了 ${repaired.length} 条收件箱记录`
              : t`没有缺失的收件箱记录`,
        });
      },
    );

  const handleOpen = () => {
    if (!selectedId) return;
    void runAndRefresh(() => commands.openInboxItem(selectedId, null));
  };

  const handleReveal = () => {
    if (!selectedId) return;
    void runAndRefresh(() => commands.showInboxItemInFolder(selectedId, null));
  };

  const handleExport = async () => {
    if (!selectedId) return;
    const destination = await pickFolder();
    if (!destination) return;
    void runAndRefresh(
      () => commands.exportInboxItem(selectedId, destination),
      t`已导出到所选位置`,
    );
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
    // selectedId/detail 由 runAndRefresh 内 loadItems 自动改选到下一条，不在此手动置空。
  };

  return (
    <main className="flex h-full flex-1 flex-col bg-transparent">
      <div className="mx-auto grid h-full w-full max-w-[1280px] gap-5 overflow-hidden p-5 lg:grid-cols-[minmax(320px,420px)_minmax(0,1fr)] lg:p-6">
        <section className="glass-panel flex min-h-0 flex-col rounded-[24px] p-4">
          <div className="flex items-start justify-between gap-3">
            <div>
              <div className="flex items-center gap-2 text-xs font-medium text-brand">
                <span className="glass-control flex size-7 items-center justify-center rounded-full">
                  <Inbox className="size-3.5" />
                </span>
                <Trans>收件箱</Trans>
              </div>
              <h1 className="mt-2 text-lg font-semibold tracking-tight text-foreground">
                <Trans>已接收内容</Trans>
              </h1>
            </div>
            <div className="flex items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="size-8"
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
              placeholder={t`搜索标题、来源、文件名…`}
              className="h-9 rounded-[14px] border-transparent bg-foreground/[0.045] pl-9 text-sm dark:bg-white/[0.05]"
            />
          </div>

          <div className="mt-3 min-h-0 flex-1 overflow-auto">
            {isSearching ? (
              searching ? (
                <ListLoading />
              ) : searchResults && searchResults.length > 0 ? (
                <div className="flex flex-col gap-2.5">
                  {searchResults.map((hit) => (
                    <SearchResultItem
                      key={hit.id}
                      hit={hit}
                      query={query}
                      selected={hit.id === selectedId}
                      onClick={() => selectItem(hit.id)}
                    />
                  ))}
                </div>
              ) : (
                <SearchEmptyState />
              )
            ) : loading ? (
              <ListLoading />
            ) : items.length === 0 ? (
              <InboxEmptyState />
            ) : (
              <div className="flex flex-col gap-2.5">
                {items.map((item) => (
                  <InboxListItem
                    key={item.id}
                    item={item}
                    selected={item.id === selectedId}
                    onClick={() => selectItem(item.id)}
                  />
                ))}
              </div>
            )}
          </div>
        </section>

        <section className="glass-panel min-h-0 overflow-hidden rounded-[24px]">
          {detail ? (
            <InboxDetail
              detail={detail}
              onOpen={handleOpen}
              onReveal={handleReveal}
              onExport={handleExport}
              onArchive={handleArchive}
              onDelete={() => setDeleteOpen(true)}
              onFileOpen={(fileId) =>
                runAndRefresh(() => commands.openInboxItem(detail.id, fileId))
              }
              onFileReveal={(fileId) =>
                runAndRefresh(() =>
                  commands.showInboxItemInFolder(detail.id, fileId),
                )
              }
            />
          ) : (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
              <Inbox className="size-8 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">
                <Trans>选择一条收件箱记录查看详情</Trans>
              </p>
            </div>
          )}
        </section>
      </div>

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
    </main>
  );
}

function InboxListItem({
  item,
  selected,
  onClick,
}: {
  item: InboxItemSummary;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full min-w-0 items-center gap-3 rounded-[18px] p-3 text-left transition-[background-color,border-color,transform] active:scale-[0.995]",
        selected
          ? "bg-primary/10 ring-1 ring-primary/20"
          : "bg-foreground/[0.035] hover:bg-foreground/[0.055] dark:bg-white/[0.045] dark:hover:bg-white/[0.065]",
      )}
    >
      <div className="glass-control flex size-10 shrink-0 items-center justify-center rounded-[14px] text-brand">
        <ItemIcon title={item.title} count={item.itemCount} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <h3 className="truncate text-sm font-medium text-foreground">
            {item.title}
          </h3>
          {item.missing && (
            <span className="shrink-0 rounded-full bg-amber-500/12 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:text-amber-300">
              <Trans>缺失</Trans>
            </span>
          )}
          {item.archivedAt && (
            <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
              <Trans>已归档</Trans>
            </span>
          )}
          {item.sourceKind === "mcp" && (
            <span className="shrink-0 rounded-full bg-primary/12 px-1.5 py-0.5 text-[10px] font-medium text-brand">
              <Trans>AI 代理</Trans>
            </span>
          )}
        </div>
        <p className="mt-0.5 truncate text-xs text-muted-foreground">
          <span>
            <Trans>来自 {item.sourceName}</Trans>
          </span>
          <span className="mx-1.5 text-foreground/25">/</span>
          <span>
            {item.itemCount} <Trans>项</Trans>
          </span>
          <span className="mx-1.5 text-foreground/25">/</span>
          <span>{formatFileSize(item.totalSize)}</span>
        </p>
      </div>
      <span className="shrink-0 text-[11px] text-muted-foreground">
        {formatRelativeTime(item.receivedAt)}
      </span>
    </button>
  );
}

function InboxDetail({
  detail,
  onOpen,
  onReveal,
  onExport,
  onArchive,
  onDelete,
  onFileOpen,
  onFileReveal,
}: {
  detail: InboxItemDetail;
  onOpen: () => void;
  onReveal: () => void;
  onExport: () => void;
  onArchive: () => void;
  onDelete: () => void;
  onFileOpen: (fileId: number) => void;
  onFileReveal: (fileId: number) => void;
}) {
  const item = detail;
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-white/25 px-5 py-4 dark:border-white/10">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h2 className="truncate text-lg font-semibold text-foreground">
              {item.title}
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              <span>
                <Trans>来自 {item.sourceName}</Trans>
              </span>
              <span className="mx-2 text-foreground/25">/</span>
              <span>{formatFileSize(item.totalSize)}</span>
              <span className="mx-2 text-foreground/25">/</span>
              <span>{new Date(item.receivedAt).toLocaleString()}</span>
            </p>
          </div>
          {item.missing && (
            <span className="flex shrink-0 items-center gap-1 rounded-full bg-amber-500/12 px-2 py-1 text-xs font-medium text-amber-700 dark:text-amber-300">
              <TriangleAlert className="size-3.5" />
              <Trans>本地内容缺失</Trans>
            </span>
          )}
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          <Button size="sm" className="gap-1.5" onClick={onOpen}>
            <ExternalLink className="size-4" />
            <Trans>打开</Trans>
          </Button>
          <Button size="sm" variant="outline" className="gap-1.5" onClick={onReveal}>
            <MapPin className="size-4" />
            <Trans>显示位置</Trans>
          </Button>
          <Button size="sm" variant="outline" className="gap-1.5" onClick={onExport}>
            <Download className="size-4" />
            <Trans>导出</Trans>
          </Button>
          <Button size="sm" variant="ghost" className="gap-1.5" onClick={onArchive}>
            {item.archivedAt ? (
              <ArchiveRestore className="size-4" />
            ) : (
              <Archive className="size-4" />
            )}
            {item.archivedAt ? <Trans>取消归档</Trans> : <Trans>归档</Trans>}
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
      </div>

      <div className="grid gap-4 overflow-auto p-5 lg:grid-cols-[minmax(0,1fr)_260px]">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-foreground">
            <Trans>文件</Trans>
          </h3>
          <div className="mt-3 flex flex-col gap-2">
            {detail.files.map((file) => (
              <div
                key={file.id}
                className="flex min-w-0 items-center gap-3 rounded-[16px] bg-foreground/[0.035] p-3 dark:bg-white/[0.045]"
              >
                <div className="glass-control flex size-9 shrink-0 items-center justify-center rounded-[12px]">
                  <ItemIcon title={file.name} count={1} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-center gap-1.5">
                    <p className="truncate text-sm font-medium text-foreground">
                      {file.relativePath}
                    </p>
                    {file.missing && (
                      <span className="rounded-full bg-amber-500/12 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-300">
                        <Trans>缺失</Trans>
                      </span>
                    )}
                  </div>
                  <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                    {file.checksum}
                  </p>
                </div>
                <span className="shrink-0 text-xs text-muted-foreground">
                  {formatFileSize(file.size)}
                </span>
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    size="icon"
                    variant="ghost"
                    className="size-8"
                    title={t`打开`}
                    onClick={() => onFileOpen(file.id)}
                  >
                    <ExternalLink className="size-4" />
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    className="size-8"
                    title={t`显示位置`}
                    onClick={() => onFileReveal(file.id)}
                  >
                    <FolderOpen className="size-4" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        </div>

        <aside className="glass-control rounded-[18px] p-4">
          <h3 className="text-sm font-semibold text-foreground">
            <Trans>来源与过程</Trans>
          </h3>
          <dl className="mt-3 space-y-3 text-xs">
            <MetaRow label={t`来源设备`} value={item.sourceName} />
            <MetaRow label={t`来源类型`} value={sourceKindLabel(item.sourceKind)} />
            <MetaRow label={t`内容类型`} value={contentKindLabel(item.contentKind)} />
            <MetaRow label={t`传输会话`} value={item.transferSessionId ?? t`已清理`} mono />
            <MetaRow label={t`本地位置`} value={item.rootPath ?? t`未知`} />
            {detail.transfer && (
              <MetaRow
                label={t`活动状态`}
                value={projectionStatusLabel(detail.transfer)}
              />
            )}
          </dl>
        </aside>
      </div>
    </div>
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
    <div>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className={cn("mt-1 break-words text-foreground", mono && "font-mono text-[11px]")}>
        {value}
      </dd>
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

function ItemIcon({ title, count }: { title: string; count: number }) {
  if (count > 1) return <FileArchive className="size-4.5 text-amber-500" />;
  const Icon = getFileIcon(title);
  return <Icon className={`size-4.5 ${getFileIconColor(title)}`} />;
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

function ListLoading() {
  return (
    <div className="flex h-full items-center justify-center">
      <Loader2 className="size-5 animate-spin text-muted-foreground" />
    </div>
  );
}

function SearchResultItem({
  hit,
  query,
  selected,
  onClick,
}: {
  hit: InboxSearchHit;
  query: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full min-w-0 items-center gap-3 rounded-[18px] p-3 text-left transition-[background-color,border-color,transform] active:scale-[0.995]",
        selected
          ? "bg-primary/10 ring-1 ring-primary/20"
          : "bg-foreground/[0.035] hover:bg-foreground/[0.055] dark:bg-white/[0.045] dark:hover:bg-white/[0.065]",
      )}
    >
      <div className="glass-control flex size-10 shrink-0 items-center justify-center rounded-[14px] text-brand">
        <ItemIcon title={hit.title} count={hit.itemCount} />
      </div>
      <div className="min-w-0 flex-1">
        <h3 className="truncate text-sm font-medium text-foreground">
          {hit.title}
        </h3>
        <p className="mt-0.5 truncate text-xs text-muted-foreground">
          <Trans>来自 {hit.sourceName}</Trans>
          {" · "}
          {hit.itemCount} <Trans>项</Trans>
        </p>
        {hit.snippet && (
          <p className="mt-1 truncate text-xs text-muted-foreground">
            <HighlightedSnippet text={hit.snippet} query={query} />
          </p>
        )}
      </div>
      <span className="shrink-0 text-[11px] text-muted-foreground">
        {formatRelativeTime(hit.receivedAt)}
      </span>
    </button>
  );
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
