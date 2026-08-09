"use client";

// 收件箱的**呈现层**：列表行、详情面板、空态、元信息胶囊。
//
// 从 `receive-panel.tsx` 抽出来的，那边只留编排（拉取、检索、归档/删除的分发、主从布局）。
// 分界线是「谁持有异步动作」：本文件收到的是已经绑好条目 id 的 `ItemAction`，只渲染它的
// pending / error / 触发——判断哪个条目该调 `archive_inbox_item(id, true)` 还是 `false`
// 留在编排层，因为那里才知道当前的归档可见性。

import { Trans, useLingui } from "@lingui/react/macro";
import { formatFileSize } from "@swarmdrop/shared-view";
import { FileBrowser } from "@swarmdrop/file-browser";
import {
  Archive,
  ArchiveRestore,
  ArrowLeftRight,
  Download,
  Inbox,
  LoaderCircle,
  MonitorSmartphone,
  Search,
  Send,
  Trash2,
} from "lucide-react";
import Link from "next/link";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import { PANEL_SURFACE, fileSectionHeightClass, selectedRowClass } from "./section";
import { ConfirmAction, INLINE_ACTION_CLASS } from "./confirm-action";
import { CenteredEmptyState } from "./empty-state";
import { ForwardToDeviceDialog } from "./forward-to-device-dialog";
import { OpenListButton } from "./master-detail";
import { RelativeTime } from "./relative-time";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import { peerLabel } from "../_lib/device-presentation";
import { itemsFromInbox } from "../_lib/file-browser-adapters";
import { NAV, inboxItemHref, transferSessionHref } from "../_lib/nav";
import { preferencesActions, usePreferences } from "../_lib/preferences-store";
import { opfsThumbnailSource } from "../_lib/thumbnail-source";
import type { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import {
  DOWNLOAD_ALL_KEY,
  INBOX_CONTENT_KIND_LABEL,
  INBOX_SOURCE_KIND_LABEL,
  inboxItemTitleLabel,
  type ItemAction,
  type InboxItemDetail,
  type InboxItemFileEntry,
} from "../_lib/view-types";

/** 检索防抖。与桌面端 `_app/inbox` 的 250ms 对齐——同一个动作在两端手感不该不同。 */
const SEARCH_DEBOUNCE_MS = 250;

/**
 * 元信息胶囊。三条并排（来源 / 内容 / 传输链接），形态一致才读得出「这是同一组事实」。
 */
function MetaChip({ children }: { children: React.ReactNode }) {
  return (
    <span className="inline-flex items-center rounded-full border px-2.5 py-0.5 text-[11px] text-muted-foreground">
      {children}
    </span>
  );
}

/**
 * 详情侧的空态。**教学文案放这里而不是列表栏**——窄屏用户落在详情屏、列表收在抽屉里，
 * 两边都摆整套空态则是宽屏下同一句话说两遍（与桌面端同一条约定）。
 */
export function InboxDetailEmpty({
  openList,
  hasRows,
}: {
  openList: (() => void) | null;
  hasRows: boolean;
}) {
  const { t } = useLingui();
  return (
    <div className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", PANEL_SURFACE)}>
      <div className="flex shrink-0 items-center gap-2 border-b px-4 py-3">
        <OpenListButton openList={openList} label={t`打开收件箱列表`} />
        {/* **区域名，不是列表栏那个集合名。** 两边都写「已接收」时，宽屏下同一个词会
            并排出现两次（列表栏一次、这里一次）——与传输页「会话」/「传输」的分法同构：
            列表栏说的是「这堆东西叫什么」，详情侧说的是「你在哪个区」。 */}
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>收件箱</Trans>
        </h2>
      </div>
      {hasRows ? (
        <CenteredEmptyState
          icon={Inbox}
          title={<Trans>选一条查看</Trans>}
          description={<Trans>选中后这里会显示它的文件、来源与可用操作。</Trans>}
        />
      ) : (
        <CenteredEmptyState
          icon={Inbox}
          title={<Trans>还没有收到的文件</Trans>}
          description={
            <Trans>对方发起传输、你接受之后，文件会落在这里，可以随时下载或归档。</Trans>
          }
          action={
            // 与传输页空态的同一颗按钮保持同一个 variant（默认 primary）。此前这里是
            // `secondary`、那边是默认——同一个动作、同一句文案、同一个去处，在两条路由上
            // 一个灰一个青绿。空态的动作是那一屏**唯一**的出口，没有理由降权。
            <Button asChild size="sm">
              <Link href={NAV.devices.href}>
                <MonitorSmartphone className="size-4" aria-hidden />
                <Trans>去设备页</Trans>
              </Link>
            </Button>
          }
        />
      )}
    </div>
  );
}

/**
 * 检索框。**防抖归它自己**，抬给面板的已经是稳定后的查询词。
 *
 * 拆出来并 `memo` 的理由是逐键重渲染：输入值留在面板里的话，每敲一个字符都要重跑整张列表
 * （N 条目 × M 文件行）——250ms 防抖只挡住了 wasm 调用，挡不住渲染。
 */
export const InboxSearchBox = memo(function InboxSearchBox({
  disabled,
  searching,
  onChange,
}: {
  disabled: boolean;
  searching: boolean;
  onChange: (query: string) => void;
}) {
  const { t } = useLingui();
  const [value, setValue] = useState("");

  useEffect(() => {
    const timer = setTimeout(() => onChange(value.trim()), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [value, onChange]);

  return (
    <div className="relative mt-3">
      <Search
        className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
        aria-hidden="true"
      />
      <input
        type="search"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        disabled={disabled}
        placeholder={t`搜索标题、来源设备或文件名`}
        aria-label={t`搜索收件箱`}
        className="w-full rounded-lg border bg-background py-2 pl-8 pr-16 text-xs text-foreground placeholder:text-muted-foreground disabled:opacity-50"
      />
      {searching && (
        <span className="absolute right-3 top-1/2 -translate-y-1/2 text-[11px] text-muted-foreground">
          <Trans>搜索中…</Trans>
        </span>
      )}
    </div>
  );
});

/**
 * 列表行 —— 只承载「认出这一条」所需的信息：未读点、标题、归档态、来源与体量、检索片段。
 * 文件清单与操作归详情侧，不在这里重复。
 *
 * 选中态由 `?item=` 承载，所以它是一条 `<Link>` 而不是按钮：可中键新开、可复制链接、
 * 刷新后还在同一条上。**必须走 `next/link`**——手写 `<a href>` 不加 basePath，
 * GitHub Pages 子路径下会 404。
 */
export function InboxListRow({
  item,
  snippet,
  selected,
  onSelect,
}: {
  item: InboxItemDetail;
  snippet: string | null;
  selected: boolean;
  onSelect: () => void;
}) {
  const { t } = useLingui();
  const unread = item.lastOpenedAt === null;
  const archived = item.archivedAt !== null;
  const ref = useRef<HTMLLIElement>(null);

  // 深链要么保证能到达，要么就别给——只换个边框色是「到达了但看不见」：列表长了，
  // 从传输页点进来的用户落在顶部，屏幕上没有任何东西变化。
  // `block: "nearest"` 让本来就在视口里的条目不跳动。
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  return (
    <li ref={ref} className="scroll-mt-4">
      <Link
        href={inboxItemHref(item.id, archived)}
        onClick={onSelect}
        aria-current={selected ? "true" : undefined}
        // 键盘导航靠它取「屏幕上的顺序」——分桶之后数据顺序与渲染顺序不再一致，
        // 查 DOM 是唯一不会漂的来源。见列表容器的 `onListKeyDown`。
        data-inbox-row
        className={cn(
          "focus-ring flex min-h-11 flex-col gap-0.5 rounded-lg border px-3 py-2 transition-colors",
          selectedRowClass(selected),
        )}
      >
        <span className="flex items-center gap-1.5 text-xs">
          {/* 未读点是「还没取走」的唯一表达——列表里其它一切在下载前后都长一样，故给 label。 */}
          {unread && <StatusDot colorClass="bg-[var(--brand-solid)]" label={t`未打开`} />}
          <span className={cn("truncate text-foreground", unread && "font-semibold")}>
            {t(inboxItemTitleLabel(item.title, item.itemCount))}
          </span>
          {archived && (
            <span className="shrink-0 rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground">
              <Trans>已归档</Trans>
            </span>
          )}
        </span>
        <span className="flex items-baseline justify-between gap-2 text-[11px] text-muted-foreground">
          <span className="truncate">
            <Trans>
              来自 {peerLabel(item.sourceName, item.sourcePeerId)} · {item.itemCount} 个文件 · {formatFileSize(item.totalSize)}
            </Trans>
          </span>
          {/* 「什么时候收到的」此前整个收件箱都没有——列表按 receivedAt 倒序排着，
              却不给任何一行标出时间，用户只能靠顺序猜。桌面端的收件箱行一直有这个位。 */}
          <RelativeTime timestamp={item.receivedAt} className="shrink-0" />
        </span>
        {/* 命中片段由 Rust 侧按子串位置切窗口生成，前端不重切——切法漂了，两端的「为什么这条
            能搜到」就对不上。与标题相同时上游已置 null（那种情况它只是把标题重复一遍）。 */}
        {snippet && (
          <span className="truncate rounded bg-muted/40 px-2 py-1 font-mono text-[11px] text-muted-foreground">
            {snippet}
          </span>
        )}
      </Link>
    </li>
  );
}

/** 详情侧 —— 选中条目的文件清单与条目级操作。 */
export function InboxDetailPanel({
  openList,
  item,
  ready,
  downloadAction,
  archive,
  remove,
  onDownload,
  onDownloadAll,
}: {
  openList: (() => void) | null;
  item: InboxItemDetail;
  ready: boolean;
  /**
   * 下载是**逐文件**的（N 个键），没法像 archive/remove 那样在父层摊平成一个值对象，
   * 故整份 handle 下传。它每次渲染都是新引用，因此本组件刻意不 `memo`——加了也会被打穿。
   */
  downloadAction: ReturnType<typeof useKeyedAsyncAction>;
  archive: ItemAction;
  remove: ItemAction;
  onDownload: (item: InboxItemDetail, file: InboxItemFileEntry) => void;
  onDownloadAll: (item: InboxItemDetail) => void;
}) {
  const { t } = useLingui();
  const archived = item.archivedAt !== null;
  const ArchiveIcon = archived ? ArchiveRestore : Archive;
  const view = usePreferences((s) => s.fileBrowserViews.inbox);
  const items = useMemo(() => itemsFromInbox(item.id, item.files), [item.id, item.files]);
  /** 逐文件下载键，与 `receive-panel` 的 `runDownload` 用的是同一个拼法。 */
  const downloadKey = (fileId: number) => `${item.id}:${fileId}`;
  const { pendingKeys } = downloadAction;
  // 这个 Set 一路传到每一行去判「我在不在下载中」，所以它的引用必须稳——否则行组件的
  // memo 全被打穿。依赖只有真正会变的两样。
  const pendingIds = useMemo(
    () =>
      new Set(
        items
          .filter((entry) => pendingKeys.has(`${item.id}:${entry.sourceId}`))
          .map((entry) => entry.id),
      ),
    [items, item.id, pendingKeys],
  );
  /**
   * **批量本身**在不在跑。判据是 `downloadAll` 自己那把 `:all` 键，不是逐文件 pending 的
   * 并集——后者会让「单点某一行的下载」也把头部按钮变成禁用的 spinner「下载中…」，
   * 界面在说「批量正在跑」而实际只有一个文件在走。
   */
  const downloading = downloadAction.isPending(`${item.id}:${DOWNLOAD_ALL_KEY}`);
  /** 有 `missing` 的文件取不回来，`downloadAll` 会跳过它们——一个都不剩时按钮不该亮着。 */
  /**
   * 还能取回的文件。「全部下载」与「发送到设备」共用这一份——两者的判据本就是同一条。
   *
   * ⚠️ 这个过滤在 Web 端目前是**空转**的：`missing` 从未被置为 `true`（`mark_file_missing`
   * 在 `docs/app/app` 下没有调用点）。留着是为了让判据与移动端同形，等 OPFS 条目被驱逐的
   * 检测补上之后它就会真正生效——见 change 的「已知限制」L3。
   */
  const usableFiles = item.files.filter((file) => !file.missing);
  const failures = item.files.flatMap((file) => {
    const error = downloadAction.errorFor(downloadKey(file.id));
    return error ? [{ file, error }] : [];
  });

  /** 待转发的文件；`null` = 对话框关着。整条与单个文件共用同一个出口。 */
  const [forwarding, setForwarding] = useState<InboxItemFileEntry[] | null>(null);

  // 动作对象也要稳：它沿 FileBrowser → 视图 → 行/卡 一路下传，内联字面量会在每一层
  // 打穿 memo。浏览器里「取回」只有下载这一种，没有「打开」「在文件夹中显示」——那两个
  // 需要真实文件系统，不传即不渲染（见 `FileBrowserActions` 的并集说明）。
  const actions = useMemo(
    () => ({
      onDownload: (entry: { sourceId?: string }) => {
        const file = item.files.find((f) => String(f.id) === entry.sourceId);
        if (file) onDownload(item, file);
      },
      onSend: (entry: { sourceId?: string }) => {
        const file = item.files.find((f) => String(f.id) === entry.sourceId);
        if (file) setForwarding([file]);
      },
      pendingIds,
    }),
    [item, onDownload, pendingIds],
  );

  return (
    // 详情自己是滚动容器：宽屏下滚它不会带走左边的列表，窄屏下也不会把页头顶走。
    // 面板级圆角走 18px 词汇，与控件的 8px 分开（DESIGN.md 的两套圆角）。
    <div className={cn("flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 sm:p-6", PANEL_SURFACE)}>
      <div className="flex items-start gap-2">
        <OpenListButton openList={openList} label={t`打开收件箱列表`} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-foreground">
            {t(inboxItemTitleLabel(item.title, item.itemCount))}
          </p>
          <p className="truncate text-xs text-muted-foreground">
            <Trans>
              来自 {peerLabel(item.sourceName, item.sourcePeerId)} · {item.itemCount} 个文件 · {formatFileSize(item.totalSize)}
            </Trans>
            {" · "}
            <RelativeTime timestamp={item.receivedAt} />
          </p>
        </div>
        {archived && (
          <span className="shrink-0 rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground">
            <Trans>已归档</Trans>
          </span>
        )}
      </div>

      {/*
        来源类型 / 内容类型 / 关联传输——三样 DTO 里一直都有（`sourceKind`、`contentKind`、
        `transfer`），Web 端此前一个都没读，而桌面三样齐全。

        「这份东西是谁、以什么身份、通过哪次传输送来的」正是收件箱要回答的问题；少了它，
        一条 AI 代理代收的文件与一条当面配对传来的文件在界面上长得一模一样。
      */}
      <div className="flex flex-wrap items-center gap-1.5">
        <MetaChip>{t(INBOX_SOURCE_KIND_LABEL[item.sourceKind])}</MetaChip>
        <MetaChip>{t(INBOX_CONTENT_KIND_LABEL[item.contentKind])}</MetaChip>
        {/* 传输页 → 收件箱的链路一直是通的，反向此前是断的（单向）。 */}
        {item.transfer && (
          <Link
            href={transferSessionHref(item.transfer.sessionId)}
            className="focus-ring inline-flex min-h-11 items-center gap-1 rounded-full border px-2.5 text-[11px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground sm:min-h-7"
          >
            <ArrowLeftRight className="size-3" aria-hidden />
            <Trans>查看传输记录</Trans>
          </Link>
        )}
      </div>

      {/* 文件清单走三端共用的 `FileBrowser`（树形 / 网格）。此前是一列扁平文件名——
          对方发一整个文件夹时，收件箱里读不出任何目录结构，而那正是「我收到了什么」
          最要紧的一半。key 绑条目：换一条时树的展开态不该漂过去。 */}
      <FileBrowser
        key={item.id}
        items={items}
        title={<Trans>文件</Trans>}
        view={view}
        onViewChange={(nextView) => preferencesActions.setFileBrowserView("inbox", nextView)}
        thumbnailSource={opfsThumbnailSource}
        actions={actions}
        // 单文件条目不给这颗按钮：那一行上就有下载，两个入口做同一件事只是噪音。
        // 全部 missing 时同样不给——它点下去一个字节也取不到。
        headerActions={
          // 两颗按钮共用一个判据，所以共用一个条件块：单文件条目一行上就有下载与转发，
          // 头部再来一遍只是噪音；全部 missing 时两者都点不出东西。此前它们各写各的
          // 条件，注释声称「判据同一条」而代码并不是——现在这句话由结构保证。
          items.length > 1 && usableFiles.length > 0 ? (
            <>
              <Button
                size="sm"
                variant="outline"
                onClick={() => setForwarding(usableFiles)}
                disabled={!ready}
                className="h-8 gap-1.5 text-xs"
              >
                <Send className="size-3.5" aria-hidden />
                <Trans>发送到设备</Trans>
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => onDownloadAll(item)}
                disabled={!ready || downloading}
                className="h-8 gap-1.5 text-xs"
              >
                {downloading ? (
                  <LoaderCircle className="size-3.5 animate-spin" aria-hidden />
                ) : (
                  <Download className="size-3.5" aria-hidden />
                )}
                {downloading ? <Trans>下载中…</Trans> : <Trans>全部下载</Trans>}
              </Button>
            </>
          ) : null
        }
        emptyState={{ title: <Trans>这条记录里没有文件</Trans> }}
        // 详情侧自己是滚动容器，本区块按内容定高；高度给**上限**不给下限，两档按视图分
        // ——理由都在 `fileSectionHeightClass` 上（与传输详情共用同一条规则）。
        className="flex-none"
        contentClassName={fileSectionHeightClass(view)}
      />

      <ForwardToDeviceDialog
        open={forwarding !== null}
        onOpenChange={(open) => {
          if (!open) setForwarding(null);
        }}
        files={forwarding ?? []}
      />

      {/* 下载失败逐条报，且带上是哪个文件——`FileBrowser` 的行里塞不下错误卡片，
          而「哪个失败了」比「有东西失败了」有用得多。 */}
      {failures.map(({ file, error }) => (
        <WebErrorCard
          key={file.id}
          error={error}
          className="text-xs"
          title={t`下载「${file.name}」失败`}
        />
      ))}

      <div className="flex flex-wrap items-center justify-end gap-2 text-xs">
        {/* 归档可逆，不设二次确认——与传输面板「续传不拦、取消才拦」同一条判据。 */}
        <button
          type="button"
          onClick={archive.run}
          disabled={!ready || archive.pending}
          className={INLINE_ACTION_CLASS}
        >
          <ArchiveIcon className="size-3" aria-hidden="true" />
          {archive.pending ? (
            <Trans>处理中</Trans>
          ) : archived ? (
            <Trans>取消归档</Trans>
          ) : (
            <Trans>归档</Trans>
          )}
        </button>
        <ConfirmAction
          icon={Trash2}
          label={t`删除`}
          pendingLabel={t`删除中`}
          confirmLabel={t`确认删除`}
          // 文案要与实际行为一致：这里删的是**记录连同浏览器里的那份文件**。
          // 已经下载到本机的那份不受影响，这点要说，否则用户会以为下载好的文件也会跟着没。
          warning={t`删除这条记录和浏览器里保存的文件；已下载到本机的副本不受影响`}
          disabled={!ready}
          pending={remove.pending}
          onConfirm={remove.run}
        />
      </div>
      {archive.error && <WebErrorCard error={archive.error} className="text-xs" />}
      {remove.error && <WebErrorCard error={remove.error} className="text-xs" />}
    </div>
  );
}
