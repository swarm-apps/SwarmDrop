"use client";

// #79 接收：入站 offer 以非阻断通知形式浮现（不做「发送/接收」Tab 切换，PRODUCT.md 原则 1）——
// accept_offer/reject_offer 决策；已接收内容在「收件箱」展示（原则 2·状态诚实可见：
// 落盘完成才给下载），点下载才读回 OPFS 建 blob URL。
// 收件箱跨刷新仍在：内核侧是一张**独立的 IndexedDB 表**（不再是「过滤已完成接收会话投影」），
// 前端经 `inbox_items()` 读它，文件本体一直在 OPFS。
// 换真表带来两处行为差异，都是修正而非回归：清空传输历史不再清掉收件箱，传输历史触到
// 100 条上限被淘汰时收件箱条目也不会跟着消失。
//
// 未配对对端的 offer 由内核在协议层硬拒 NotPaired，从不进入 `pending_offers()`——本机对此
// 零可见性（符合「安全边界」设计，不是本面板的缺陷）。#79 验收标准里的「需先配对」提示因此
// 落在发送侧：见 send-panel.tsx 消费 `rejections` 域。
//
// #96 拆成两块并挂到 /app/inbox：待处理请求是**决策**（有时效），收件箱是**结果**（可回看）。
// 两者混在一个卡里时，一条永久列表会把一条限时动作压下去。多路由后请求的可见性由导航徽标
// 兜底（见 app-nav.tsx），用户不在收件箱页也知道有东西等着。
//
// #108 收件箱从「只能滚的列表」补成可用：检索 / 已读 / 归档 / 删除四件事各自接上内核既有的
// 导出。此前那五个方法是完整实现却零调用方——能力在内核里，UI 上不存在。

import { Trans, useLingui } from "@lingui/react/macro";
import { formatFileSize } from "@swarmdrop/shared-view";
import { Archive, ArchiveRestore, Search, Trash2 } from "lucide-react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Suspense, memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import { ConfirmAction, INLINE_ACTION_CLASS } from "./confirm-action";
import { MasterDetail, OpenListButton } from "./master-detail";
import { PanelFallback } from "./panel-fallback";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import { inboxItemHref, PARAM } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import {
  toWebError,
  type InboxItemDetail,
  type InboxItemFileEntry,
  type InboxSearchHit,
  type WebError,
} from "../_lib/view-types";

/**
 * blob URL 的存活窗口。
 *
 * 一个 blob URL 会钉住它背后那份 OPFS 文件快照直到被 revoke 或页面关闭——收件箱累计几个 GB，
 * 就是几个 GB 的浏览器存储回收不掉。所以下载链接**点了才生成、用完就撤**，而不是进页面就把
 * 整个收件箱解析一遍（#81 把历史灌进 projections 后，那会变成上百次 OPFS 句柄操作 + 上百个
 * 永不回收的 URL）。撤销给足 30 秒：`a.click()` 触发后浏览器读取 blob 是异步的，立刻 revoke
 * 会让大文件下载中途失败。
 */
const BLOB_URL_TTL_MS = 30_000;

/** 检索防抖。与桌面端 `_app/inbox` 的 250ms 对齐——同一个动作在两端手感不该不同。 */
const SEARCH_DEBOUNCE_MS = 250;

export function IncomingOffersPanel() {
  const offers = useWebNode((s) => s.offers);
  const decideAction = useKeyedAsyncAction();

  const offerList = Object.values(offers);

  const decide = (sessionId: string, accept: boolean) => {
    const node = getNode();
    if (!node) return;
    void decideAction.run(sessionId, async () => {
      if (accept) await node.accept_offer(sessionId);
      else await node.reject_offer(sessionId);
      webNodeActions.removeOffer(sessionId);
    });
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>待处理请求</Trans>
        </h2>
        {offerList.length > 0 && (
          <p
            className="rounded-full bg-fd-accent px-2 py-0.5 text-xs font-medium text-fd-foreground"
            role="status"
            aria-live="polite"
          >
            <Trans>{offerList.length} 个待处理</Trans>
          </p>
        )}
      </div>

      {offerList.length === 0 ? (
        <p className="mt-2 text-xs text-muted-foreground">
          <Trans>暂无待处理的入站文件请求。</Trans>
        </p>
      ) : (
        <>
        {/* 请求到达时会先弹全局对话框（`transfer-offer-host.tsx`）。这里是**关掉之后**
            的回看入口——说清楚，否则同一件事出现在两个地方会让人以为是重复。 */}
        <p className="mt-2 text-xs text-muted-foreground">
          <Trans>你关掉提示后，请求会留在这里等你决定。</Trans>
        </p>
        <ul className="mt-3 space-y-2">
          {offerList.map((offer) => (
            <li key={offer.sessionId} className="rounded-lg border border-fd-border bg-fd-background px-3 py-2">
              <p className="text-xs text-foreground">
                <Trans>
                  <span className="font-medium">{offer.deviceName}</span> 想发送{" "}
                  {offer.files.length} 个文件（{formatFileSize(offer.totalSize)}）
                </Trans>
              </p>
              <ul className="mt-1 space-y-0.5">
                {offer.files.map((f) => (
                  <li key={f.fileId} className="truncate font-mono text-[11px] text-fd-muted-foreground">
                    {f.name}（{formatFileSize(f.size)}）
                  </li>
                ))}
              </ul>
              <div className="mt-2 flex gap-2">
                <Button
                  size="sm"
                  onClick={() => decide(offer.sessionId, true)}
                  disabled={decideAction.isPending(offer.sessionId)}
                  className="min-h-9"
                >
                  <Trans>接受</Trans>
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => decide(offer.sessionId, false)}
                  disabled={decideAction.isPending(offer.sessionId)}
                  className="min-h-9"
                >
                  <Trans>拒绝</Trans>
                </Button>
              </div>
            </li>
          ))}
        </ul>
        </>
      )}
      {decideAction.latestError && <WebErrorCard error={decideAction.latestError} className="mt-2 text-xs" />}
    </div>
  );
}

/** 一行收件箱：完整条目 + 检索态下 Rust 侧切出的命中片段。 */
type InboxRow = {
  item: InboxItemDetail;
  snippet: string | null;
};

/** 一个条目级动作的全部对外状态。与 transfer-activity-panel 的 `ItemAction` 同形。 */
type ItemAction = {
  pending: boolean;
  error: WebError | undefined;
  run: () => void;
};

/** 读 `?item=` / `?archived=`（传输页反查过来的定位），故需要 Suspense 边界——静态导出下没有它 build 会红。 */
export function InboxPanel() {
  return (
    <Suspense fallback={<PanelFallback>
        <Trans>正在打开收件箱…</Trans>
      </PanelFallback>}>
      <InboxPanelInner />
    </Suspense>
  );
}

function InboxPanelInner() {
  const { t } = useLingui();
  const items = useWebNode((s) => s.inboxItems);
  const status = useWebNode((s) => s.status);
  const inboxRevision = useWebNode((s) => s.inboxRevision);
  const searchLimit = useWebNode((s) => s.inboxSearchLimit);
  const searchParams = useSearchParams();
  const focusedId = searchParams.get(PARAM.item);

  const archivedParam = searchParams.get(PARAM.archived) === "1";
  // 归档可见性的初值来自 URL：只带 id 的链接到不了已归档的目标（见 `inboxItemHref`）。
  // 勾选状态本身不写回 URL——检索词与勾选都只是本地视图状态，Web 端的 URL 目前只承担跨页深链。
  const [showArchived, setShowArchived] = useState(archivedParam);
  /** 已防抖的查询词（输入框自己防抖后才抬上来，见 `InboxSearchBox`）。 */
  const [query, setQuery] = useState("");
  /** `null` = 非检索态（展示全部）；空数组 = 检索了但零命中。两者的空态文案不同。 */
  const [hits, setHits] = useState<InboxSearchHit[] | null>(null);
  const [loadError, setLoadError] = useState<WebError | null>(null);
  /**
   * 首次拉取是否已回。只用来压住「要定位的条目不在当前视图」那条提示——`items` 初值是空数组，
   * 少了它，从传输页跳进来的用户会先看到一帧「找不到」再看到条目。
   */
  const [loaded, setLoaded] = useState(false);
  /** 本次挂载期间已标记过已读的条目，防同一条目多文件并发下载各标记一次（见 `download`）。 */
  const markedOpened = useRef<Set<string>>(new Set());
  const searchAction = useAsyncAction();
  const downloadAction = useKeyedAsyncAction();
  const itemAction = useKeyedAsyncAction();

  const ready = status === "running";

  // 同路由内 query 变化**不重挂组件**，所以初值之外还要跟着 param 走（知识库「静态导出的三条
  // 硬限制」第 3 条记的正是这个坑）。今天 `?item=` 的唯一生产者在 `/app/transfer`，两次跳转
  // 之间本页会卸载重挂，看似不需要——但那是导航图的偶然，不是机制保证：谁在收件箱内部加一条
  // 「在列表中定位」的链接，脱钩就发生了。
  //
  // **单向**：只负责打开，不负责关。用户手动勾上之后点一条不带 `archived` 的链接，不该把他
  // 正在看的视图关掉。
  useEffect(() => {
    if (archivedParam) setShowArchived(true);
  }, [archivedParam]);

  // 唯一的拉取点：挂载（节点就绪后）/ 收完一次 / 本机改过收件箱（`refreshInbox()` 顶计数器）。
  //
  // **恒拉全集**（`include_archived = true`），归档可见性由下面的 `rows` 在内存里过滤。
  // 让 `showArchived` 参与这里会让勾一次开关就多一次 wasm 往返 + 一次全量重排，而那次重拉
  // 买不到任何新数据。前端过滤在这里是安全的：判据只是 `archivedAt` 非空，没有会漂的语义
  // ——与检索命中判据不同，后者必须留在内核（见下）。
  useEffect(() => {
    if (!ready) return;
    let cancelled = false;
    void (async () => {
      const node = getNode();
      if (!node) return;
      try {
        const details = await node.inbox_items(true);
        if (cancelled) return;
        webNodeActions.setInboxItems(details);
        setLoadError(null);
      } catch (e) {
        if (cancelled) return;
        console.error("[web] inbox_items() 失败", e);
        setLoadError(toWebError(e));
      } finally {
        if (!cancelled) setLoaded(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [ready, inboxRevision]);

  // 检索走内核的 `search_inbox`，**不在前端 filter 这个内存数组**。命中判据在
  // `crates/transfer/src/inbox.rs::inbox_matches` 有规范定义（大小写、四列覆盖面、
  // `%`/`_` 转义），桌面端与 Web 端共用同一份 conformance 语料；前端自己写一套
  // `includes()` 必然与它漂开，而漂开的症状是「同一批数据两端搜出不同结果」。
  //
  // 退出检索态必须**显式作废**在途请求：`run` 的 seq 只覆盖「新一轮顶掉旧一轮」，清空搜索框
  // 不发起新调用，旧 promise resolve 时会把结果写回一个已经不该显示它的界面（顺带让上一次的
  // 报错卡赖在非检索态的列表顶上）。Web 侧 `search_inbox` 是纯内存扫描、几乎同 tick 就 resolve，
  // 所以这条路径靠时序侥幸也能对——但那是侥幸不是机制。
  //
  // `inboxRevision` 在依赖里：检索期间收到新文件，它该进得了当前命中集合。
  // `searchAction` 不进依赖：`useAsyncAction` 每次渲染返回新对象，放进去就是无限循环。
  useEffect(() => {
    if (!query) {
      searchAction.cancel();
      setHits(null);
      return;
    }
    if (!ready) return;
    const node = getNode();
    if (!node) return;
    // limit 传 undefined：条数上限是三端共享的 `INBOX_SEARCH_LIMIT`，由内核兜底。
    // 前端各带一个魔数正是 #111 修掉的分叉（此前桌面 20 / 移动 100 / 这里 50，而截断的
    // 永远是最早收到的那批，于是同一个词在两端搜出不同结果）。
    searchAction.run(() => node.search_inbox(query, undefined, showArchived), setHits);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, showArchived, ready, inboxRevision]);

  // 检索命中只取 id，再映射回已加载的完整条目。
  //
  // `InboxSearchHit` 是**另一种形状**：有 snippet，但没有文件大小、没有 `lastOpenedAt` /
  // `archivedAt`。直接拿它渲染会让检索态少掉未读点、归档按钮与每个文件的体积——同一个列表
  // 在搜与不搜之间能力不同。而按 id 逐条 `inbox_item()` 补详情就退回了 1+N（那正是这轮
  // 端口重构刚删掉的东西）。收件箱是全内存表、列表已经在手，映射一次就够。
  const rows = useMemo<InboxRow[]>(() => {
    const visible = showArchived ? items : items.filter((item) => item.archivedAt === null);
    if (!hits) return visible.map((item) => ({ item, snippet: null }));
    const byId = new Map(visible.map((item) => [item.id, item]));
    return hits.flatMap((hit) => {
      const item = byId.get(hit.id);
      // snippet 是否该渲染由内核判（命中标题/来源名时给 `null`——那两样条目行上已经显示着）。
      // 前端一度自己比对字符串，那份判据编码了 `inbox_snippet` 的窗口半径与省略号规则，
      // 改内核就会静默失效；#111 把它推回领域层后这里只剩透传。
      return item ? [{ item, snippet: hit.snippet }] : [];
    });
  }, [hits, items, showArchived]);

  const isSearching = hits !== null;
  /**
   * 定位条目的三态。**「看不见」不等于「没有」**——归档只是可见性开关，说成「已不在收件箱」
   * 就是对用户撒谎，而它恰恰会在用户自己刚归档这一条时触发（入口处的 `?archived=` 只兜住了
   * 进来那一刻，兜不住会话中途的归档）。判据所需的数据一直在手：`items` 恒含已归档条目。
   */
  const focusState = useMemo<"visible" | "filtered-by-search" | "archived" | "gone">(() => {
    if (rows.some((row) => row.item.id === focusedId)) return "visible";
    const item = items.find((i) => i.id === focusedId);
    if (!item) return "gone";
    if (item.archivedAt !== null && !showArchived) return "archived";
    return isSearching ? "filtered-by-search" : "gone";
  }, [rows, items, focusedId, showArchived, isSearching]);

  /** 三个条目级动作的共同外壳：取节点、跑 keyed action、成功后请求重拉。 */
  const runOnItem = useCallback(
    (key: string, call: (node: NonNullable<ReturnType<typeof getNode>>) => Promise<void>) => {
      const node = getNode();
      if (!node) return;
      void itemAction.run(key, async () => {
        await call(node);
        webNodeActions.refreshInbox();
      });
    },
    [itemAction.run],
  );

  const download = useCallback(
    (item: InboxItemDetail, file: InboxItemFileEntry) => {
      const node = getNode();
      if (!node) return;
      void downloadAction.run(`${item.id}:${file.id}`, async () => {
        const url = await node.download_url(file.relativePath).catch((e: unknown) => {
          // 控制台留一条带 relativePath 的诊断：错误卡片上只有一句话，定位不到是哪份 OPFS 路径。
          console.error(`[web] download_url(${file.relativePath}) 失败`, e);
          throw e;
        });
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = file.name;
        anchor.click();
        setTimeout(() => URL.revokeObjectURL(url), BLOB_URL_TTL_MS);

        // 「打开过」= 用户真的把内容取走了，与桌面端 `open_inbox_item`（用系统程序打开文件）
        // 同义。绑在「进过收件箱页」上会让未读点在第一次滚动时集体消失，那个标记就不再表达
        // 任何东西。
        //
        // 去重用本地 ref 而不是读 store：store 要变新值得走完
        // `mark` → `refreshInbox` → 重渲染 → 拉取 effect → `inbox_items()`，中间隔着两个渲染
        // 周期。同一条目下连点两个文件时，两个 `download_url`（OPFS 读，几十到几百毫秒）几乎
        // 同时 resolve，读 store 的话两个闭包看到的都还是 `null`。ref 在同步那一拍就闭合。
        if (markedOpened.current.has(item.id) || item.lastOpenedAt !== null) return;
        markedOpened.current.add(item.id);
        try {
          await node.mark_inbox_item_opened(item.id);
          webNodeActions.refreshInbox();
        } catch (e) {
          // 下载本身已经成功，未读点没消掉不值得把这一行标红——那会让用户以为文件没拿到。
          // 但要放行重试：标记失败还占着去重位，这条就再也标不上了。
          markedOpened.current.delete(item.id);
          console.error("[web] mark_inbox_item_opened() 失败", e);
        }
      });
    },
    [downloadAction.run],
  );

  /** 当前选中的条目。`?item=` 既是深链参数也是选中态——不再多引一个本地 state。 */
  const selected = useMemo(
    () => rows.find((row) => row.item.id === focusedId) ?? null,
    [rows, focusedId],
  );

  return (
    <MasterDetail
      testId="inbox-master-detail"
      drawerLabel={t`收件箱列表`}
      list={({ closeDrawer }) => (
        <div className="flex min-h-0 flex-col gap-3 p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-sm font-semibold text-foreground">
              <Trans>已接收</Trans>
            </h2>
            <label className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground">
              <input
                type="checkbox"
                checked={showArchived}
                onChange={(e) => setShowArchived(e.target.checked)}
              />
              <Trans>显示已归档</Trans>
            </label>
          </div>

          <InboxSearchBox disabled={!ready} searching={searchAction.pending} onChange={setQuery} />

          {loadError && <WebErrorCard error={loadError} className="text-xs" />}
          {searchAction.error && <WebErrorCard error={searchAction.error} className="text-xs" />}

          {/* 定位条目不在视图时给一句解释，而不是让用户对着一个「点了没反应」的链接发呆。
              「被归档滤掉」这一态给逃生按钮——那不是补偿，是把「归档只是可见性开关」这条领域
              规则在 UI 上兑现；条目真没了则只陈述事实，没有出路可给。 */}
          {focusedId !== null && loaded && focusState !== "visible" && !loadError && (
            <p className="rounded-lg border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
              {focusState === "archived" ? (
                <>
                  <Trans>要定位的条目已归档。</Trans>
                  <button
                    type="button"
                    onClick={() => setShowArchived(true)}
                    className="ml-1 cursor-pointer font-medium text-foreground underline underline-offset-2"
                  >
                    <Trans>显示已归档</Trans>
                  </button>
                </>
              ) : focusState === "filtered-by-search" ? (
                <Trans>要定位的条目不在当前检索结果里。</Trans>
              ) : (
                <Trans>要定位的条目已不在收件箱。</Trans>
              )}
            </p>
          )}

          {rows.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              {isSearching ? (
                <Trans>没有匹配的收件箱条目。</Trans>
              ) : showArchived ? (
                <Trans>还没有收到的文件。</Trans>
              ) : (
                <Trans>还没有收到的文件（已归档的未显示）。</Trans>
              )}
            </p>
          ) : (
            <ul className="flex min-h-0 flex-col gap-1.5 overflow-y-auto">
              {/* 截断要说出来。命中正好等于上限时无法区分「刚好这么多」与「还有更多」，
                  所以文案取保守说法——比静默丢掉让用户以为搜全了要诚实。
                  上限从内核读（`inbox_search_limit()`），前端不自带这个数字。 */}
              {isSearching && searchLimit !== null && hits.length >= searchLimit && (
                <li className="text-[11px] text-muted-foreground">
                  <Trans>
                    只显示最近 {searchLimit} 条匹配，更早的未列出——请把关键词写得更具体。
                  </Trans>
                </li>
              )}
              {rows.map(({ item, snippet }) => (
                <InboxListRow
                  key={item.id}
                  item={item}
                  snippet={snippet}
                  selected={item.id === focusedId}
                  onSelect={closeDrawer}
                />
              ))}
            </ul>
          )}
        </div>
      )}
      detail={({ openList }) =>
        selected ? (
          <InboxDetailPanel
            openList={openList}
            item={selected.item}
            ready={ready}
            downloadAction={downloadAction}
            archive={{
              pending: itemAction.isPending(`${selected.item.id}:archive`),
              error: itemAction.errorFor(`${selected.item.id}:archive`),
              run: () =>
                runOnItem(`${selected.item.id}:archive`, (node) =>
                  node.archive_inbox_item(selected.item.id, selected.item.archivedAt === null),
                ),
            }}
            remove={{
              pending: itemAction.isPending(`${selected.item.id}:delete`),
              error: itemAction.errorFor(`${selected.item.id}:delete`),
              // **总是连文件一起删**（第二参恒 true），不像桌面那样给「保留本地文件」开关。
              //
              // 那个开关在桌面有意义：文件躺在用户的文件系统里，删了记录还能用文件管理器
              // 打开。OPFS 没有这层——记录一软删，`list`/`search`/`detail` 全看不到它，
              // 那份副本就**永久不可达也不可清理**，配额却一直占着。于是「只删记录」在
              // 浏览器上唯一的效果就是泄漏，把它做成选项是让用户选一个没有好处的坑。
              //
              // 这不是与桌面分叉：同一个开关在两端的**含义**不同（那边是「留着还能用」，
              // 这边是「留着但谁也碰不到」），行为跟着含义走才是对齐。
              run: () =>
                runOnItem(`${selected.item.id}:delete`, (node) =>
                  node.delete_inbox_item(selected.item.id, true),
                ),
            }}
            onDownload={download}
          />
        ) : (
          <InboxDetailEmpty openList={openList} hasRows={rows.length > 0} />
        )
      }
    />
  );
}

/**
 * 详情侧的空态。**教学文案放这里而不是列表栏**——窄屏用户落在详情屏、列表收在抽屉里，
 * 两边都摆整套空态则是宽屏下同一句话说两遍（与桌面端同一条约定）。
 */
function InboxDetailEmpty({
  openList,
  hasRows,
}: {
  openList: (() => void) | null;
  hasRows: boolean;
}) {
  const { t } = useLingui();
  return (
    <div className="flex flex-col gap-3 rounded-xl border bg-card p-6 shadow-xs">
      <div className="flex items-center gap-2">
        <OpenListButton openList={openList} label={t`打开收件箱列表`} />
        <h2 className="text-sm font-semibold text-foreground">
              <Trans>已接收</Trans>
            </h2>
      </div>
      <p className="text-xs text-muted-foreground">
        {hasRows ? (
          <Trans>从左侧选一条，这里会显示它的文件与操作。</Trans>
        ) : (
          <Trans>还没有收到的文件。对方发起传输并被你接受后，文件会出现在这里。</Trans>
        )}
      </p>
    </div>
  );
}

/**
 * 检索框。**防抖归它自己**，抬给面板的已经是稳定后的查询词。
 *
 * 拆出来并 `memo` 的理由是逐键重渲染：输入值留在面板里的话，每敲一个字符都要重跑整张列表
 * （N 条目 × M 文件行）——250ms 防抖只挡住了 wasm 调用，挡不住渲染。
 */
const InboxSearchBox = memo(function InboxSearchBox({
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
        className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-fd-muted-foreground"
        aria-hidden="true"
      />
      <input
        type="search"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        disabled={disabled}
        placeholder={t`搜索标题、来源设备或文件名`}
        aria-label={t`搜索收件箱`}
        className="w-full rounded-lg border border-fd-border bg-fd-background py-2 pl-8 pr-16 text-xs text-fd-foreground placeholder:text-fd-muted-foreground disabled:opacity-50"
      />
      {searching && (
        <span className="absolute right-3 top-1/2 -translate-y-1/2 text-[11px] text-fd-muted-foreground">
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
function InboxListRow({
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
        className={cn(
          "flex min-h-11 flex-col gap-0.5 rounded-lg border px-3 py-2 transition-colors",
          selected
            ? "border-[var(--brand)]/40 bg-accent ring-1 ring-[var(--brand)]/20"
            : "hover:bg-accent",
        )}
      >
        <span className="flex items-center gap-1.5 text-xs">
          {/* 未读点是「还没取走」的唯一表达——列表里其它一切在下载前后都长一样，故给 label。 */}
          {unread && <StatusDot colorClass="bg-[var(--brand-solid)]" label={t`未打开`} />}
          <span className={cn("truncate text-foreground", unread && "font-semibold")}>
            {item.title}
          </span>
          {archived && (
            <span className="shrink-0 rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground">
              <Trans>已归档</Trans>
            </span>
          )}
        </span>
        <span className="truncate text-[11px] text-muted-foreground">
          <Trans>
            来自 {item.sourceName} · {item.itemCount} 个文件 · {formatFileSize(item.totalSize)}
          </Trans>
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
function InboxDetailPanel({
  openList,
  item,
  ready,
  downloadAction,
  archive,
  remove,
  onDownload,
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
}) {
  const { t } = useLingui();
  const archived = item.archivedAt !== null;
  const ArchiveIcon = archived ? ArchiveRestore : Archive;

  return (
    <div className="flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-xs sm:p-6">
      <div className="flex items-start gap-2">
        <OpenListButton openList={openList} label={t`打开收件箱列表`} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-foreground">{item.title}</p>
          <p className="truncate text-xs text-muted-foreground">
            <Trans>
              来自 {item.sourceName} · {item.itemCount} 个文件 · {formatFileSize(item.totalSize)}
            </Trans>
          </p>
        </div>
        {archived && (
          <span className="shrink-0 rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground">
            已归档
          </span>
        )}
      </div>

      <ul className="flex flex-col gap-1.5">
        {item.files.map((f) => {
          const key = `${item.id}:${f.id}`;
          const error = downloadAction.errorFor(key);
          return (
            <li key={f.id} className="rounded-lg border bg-background px-3 py-2 text-xs">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-foreground">{f.name}</span>
                <span className="flex shrink-0 items-center gap-2">
                  <span className="font-mono tabular-nums text-muted-foreground">
                    {formatFileSize(f.size)}
                  </span>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={() => onDownload(item, f)}
                    disabled={downloadAction.isPending(key)}
                    className="min-h-9"
                  >
                    {downloadAction.isPending(key) ? (
                      <Trans>准备中…</Trans>
                    ) : error ? (
                      <Trans>重试下载</Trans>
                    ) : (
                      <Trans>下载</Trans>
                    )}
                  </Button>
                </span>
              </div>
              {error && <WebErrorCard error={error} className="mt-1 text-xs" />}
            </li>
          );
        })}
      </ul>

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
