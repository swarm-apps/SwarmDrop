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

import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { formatFileSize, groupByTimeBucket } from "@swarmdrop/shared-view";
import type { FileBrowserTarget } from "@swarmdrop/file-browser";
import { useSearchParams } from "next/navigation";
import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { MasterDetail } from "./master-detail";
// 呈现层（列表行 / 详情 / 空态 / 检索框）住在 `inbox-views.tsx`——本文件只做编排。
import {
  InboxDetailEmpty,
  InboxDetailPanel,
  InboxListRow,
  InboxSearchBox,
} from "./inbox-views";
import { PanelFallback } from "./panel-fallback";
import { WebErrorCard } from "./web-error-view";
import { PARAM } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import {
  TIME_BUCKET_LABEL,
  allDownloadKey,
  directoryDownloadKey,
  fileDownloadKey,
  toWebError,
  usableInboxFiles,
  type InboxItemDetail,
  type InboxItemFileEntry,
  type InboxSearchHit,
  type WebError,
} from "../_lib/view-types";
import {
  ZIP_SIZE_WARN_BYTES,
  ZipAbortedError,
  buildZip,
  planDirectoryZip,
  planItemZip,
  type ZipPlan,
} from "../_lib/zip-download";

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

/**
 * 把一个 object URL 交给浏览器下载，并在 TTL 后撤销。单文件与 zip 共用**同一个 TTL**。
 *
 * ⚠️ 曾经给 zip 单独调过一个更短的（3s），理由是「整包已物化、占内存」——**那是把
 * 收益侧当成了判据**。TTL 存在的原因写在 `BLOB_URL_TTL_MS` 上：revoke 太早会让下载
 * 中途失败（Firefox 在下载进行中 revoke 会直接中断）。而 zip 恰恰是体积最大、
 * 浏览器落盘最慢的那一类——最不能提前 revoke 的就是它。省内存不值得拿一个 0 字节的
 * .zip 去换，何况 UI 那时已经说了「已开始下载」。
 */
function saveObjectUrl(url: string, fileName: string) {
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), BLOB_URL_TTL_MS);
}

/**
 * 取回一个文件：读 OPFS 建 blob URL → 触发浏览器下载 → 延迟撤销。
 *
 * 抛出的错误交给调用方的 keyed action 落到对应那一行。
 */
async function triggerDownload(
  node: NonNullable<ReturnType<typeof getNode>>,
  file: InboxItemFileEntry,
) {
  const url = await node.download_url(file.relativePath).catch((e: unknown) => {
    // 控制台留一条带 relativePath 的诊断：错误卡片上只有一句话，定位不到是哪份 OPFS 路径。
    console.error(`[web] download_url(${file.relativePath}) 失败`, e);
    throw e;
  });
  saveObjectUrl(url, file.name);
}


/** 一行收件箱：完整条目 + 检索态下 Rust 侧切出的命中片段。 */
type InboxRow = {
  item: InboxItemDetail;
  snippet: string | null;
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
  /**
   * 有没有打包正在跑。**不读 `pendingKeys`**：那是 state，`setState` 要等下一个渲染
   * 周期才可见，而两次点击可以落在同一拍里；ref 在同步那一拍就闭合（与 `markedOpened`
   * 同一条理由）。
   */
  const zipInFlight = useRef(false);
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
   * 按时间分桶。分桶逻辑住在 `@swarmdrop/shared-view`（`groupByTimeBucket`），与桌面同一份
   * ——桶的边界（今天从几点起、本周算几天）三端不该有不同答案。
   *
   * 检索态不分桶（下面的渲染分支跳过它）：命中按相关度排列，套上时间组头会暗示一个并不
   * 存在的时序。
   */
  const groupedRows = useMemo(
    () => groupByTimeBucket(rows, (row) => row.item.receivedAt),
    [rows],
  );

  /**
   * ↑/↓ 在列表里移动。桌面一直有，Web 端此前只能用鼠标。
   *
   * **移动的是焦点，不是选中态**：每一行本身就是 `<Link>`，选中由 URL 承载。把焦点挪到
   * 相邻那条链接、让 Enter 走浏览器原生的「激活链接」，比自己算下一个 id 再 `router.replace`
   * 少一套需要与渲染顺序保持同步的状态——分桶之后 `rows` 的顺序与屏幕顺序已经不一致，
   * 那套算法会让按一次下键跳到别处去。
   *
   * 用 DOM 查询取顺序也就顺带正确处理了检索态与分组态的差异：屏幕上是什么顺序就是什么顺序。
   */
  const listRef = useRef<HTMLDivElement>(null);
  const onListKeyDown = useCallback((event: React.KeyboardEvent) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const links = Array.from(
      listRef.current?.querySelectorAll<HTMLAnchorElement>("a[data-inbox-row]") ?? [],
    );
    if (links.length === 0) return;
    event.preventDefault();
    const current = links.indexOf(document.activeElement as HTMLAnchorElement);
    const delta = event.key === "ArrowDown" ? 1 : -1;
    const next =
      current < 0
        ? delta > 0
          ? 0
          : links.length - 1
        : Math.min(links.length - 1, Math.max(0, current + delta));
    // `block: "nearest"` 让本来就在视口里的条目不跳动。
    links[next]?.focus();
    links[next]?.scrollIntoView({ block: "nearest" });
  }, []);

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

  /**
   * 「打开过」= 用户真的把内容取走了，与桌面端 `open_inbox_item`（用系统程序打开文件）
   * 同义。绑在「进过收件箱页」上会让未读点在第一次滚动时集体消失，那个标记就不再表达
   * 任何东西。
   *
   * 去重用本地 ref 而不是读 store：store 要变新值得走完
   * `mark` → `refreshInbox` → 重渲染 → 拉取 effect → `inbox_items()`，中间隔着两个渲染
   * 周期。同一条目下连点两个文件时，两个 `download_url`（OPFS 读，几十到几百毫秒）几乎
   * 同时 resolve，读 store 的话两个闭包看到的都还是 `null`。ref 在同步那一拍就闭合。
   */
  const markOpened = useCallback(async (item: InboxItemDetail) => {
    if (markedOpened.current.has(item.id) || item.lastOpenedAt !== null) return;
    const node = getNode();
    if (!node) return;
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
  }, []);

  const download = useCallback(
    (item: InboxItemDetail, file: InboxItemFileEntry) => {
      const node = getNode();
      if (!node) return;
      void downloadAction.run(fileDownloadKey(item.id, file.id), async () => {
        await triggerDownload(node, file);
        // 下载是**结果完全不在本页**的动作：文件去了浏览器的下载目录，而下载栏在不少
        // 浏览器/设置下根本不弹。没有这条确认，用户点完只看到按钮闪了一下，无从判断
        // 是成功了还是什么都没发生。
        toast.success(t`已开始下载 ${file.name}`);
        await markOpened(item);
      });
    },
    [downloadAction.run, markOpened, t],
  );

  /**
   * 取回一**组**文件：打成 zip 下载。「全部下载」与目录下载共用这一份。
   *
   * **它取代了「循环触发 N 次 `<a download>`」那条路**，后者在浏览器上根本走不完：第二次起
   * 的程序化下载会撞上「是否允许下载多个文件」，而 `anchor.click()` 跑在 `await` 之后早已
   * 脱离用户手势，被拦时既不回调也不抛错——落盘的只有最前面几个。而收件箱的文件顺序是
   * `file_id` 升序 = 发送侧 walkdir 顺序，被吃掉的恰好是**深层**那些，线上症状正是
   * 「目录只下载了第一层」。`<a download>` 还会消毒掉文件名里的路径分隔符，就算全下来了
   * 层级也留不住。完整推导见 `_lib/zip-download.ts` 的模块注释。
   *
   * **跳过 `missing` 的文件**：逐行的下载按钮对它们是明确 disabled 的，打包却照收的话，
   * 用户会拿到一个悄悄少了几个文件的压缩包。
   *
   * `lastModified` 用条目的接收时间而不是「现在」：zip 里的时间戳应当描述这批文件，
   * 而不是描述用户点按钮的那一刻（后者每次下载都不同，同一份内容打两次得到两个包）。
   */
  const runZipDownload = useCallback(
    (item: InboxItemDetail, key: string, plan: ZipPlan) => {
      // **同一时刻只允许一个打包在跑。** 每个包都是一次性物化的整包字节，两次并发就是
      // 两份同时驻留（「全部下载」+ 某个子目录 = 整条记录 + 它的一部分）；而两颗按钮
      // 各按各的 pending 判 disabled，界面上拦不住这个组合。
      if (zipInFlight.current) {
        toast.info(t`正在打包上一个下载，请稍候`);
        return;
      }
      zipInFlight.current = true;
      void downloadAction.run(key, async () => {
        // 空计划也要走进 keyed action 才报得出来。**在外面提前 return 是静默的**：
        // 目录里的文件全 missing 时，用户点了目录的下载按钮会一点反馈都没有
        // （没有 spinner、没有 toast、没有错误卡片）——而那颗按钮看起来是可点的。
        if (plan.entries.length === 0) {
          throw new Error(t`这里没有可取回的文件`);
        }
        // 大包**开工前**就要提醒：整包是一次性物化的，真崩也是在读完所有 OPFS 字节之后
        // 才崩——那时用户已经白等了很久。只提醒不拦截（见 `ZIP_SIZE_WARN_BYTES`）。
        if (plan.totalSize > ZIP_SIZE_WARN_BYTES) {
          const size = formatFileSize(plan.totalSize);
          toast.warning(t`正在打包 ${size}，大文件可能超出浏览器限制`, {
            description: t`如果失败，可以改为逐个文件下载。`,
          });
        }
        const { blob, skipped } = await buildZip(
          plan,
          // **每次重取节点句柄**，不在外面捕获一次：用户可以在打包进行中从节点状态弹窗
          // 停掉节点（`_lib/node-lifecycle.ts` 的 `closeNode`），此后 wasm 对象已释放。
          // 抛 `ZipAbortedError` 而不是普通错误，`buildZip` 才不会把它当成「这个文件
          // 取不到」吞掉——那样用户会拿到一个静默截断的包，外加一句说反了的归因。
          (path) => {
            const node = getNode();
            if (!node) throw new ZipAbortedError(t`节点已停止，打包中断`);
            return node.open_file(path);
          },
          new Date(item.receivedAt),
        );
        if (skipped.length === plan.entries.length) {
          // 一个都没取到时包里只剩一个空的中央目录（22 字节）。给用户一个 22 字节的 zip
          // 比报错更糟——他会以为文件真的没了，而实际多半只是 OPFS 条目被浏览器驱逐。
          throw new Error(t`这些文件已不在浏览器存储中，无法打包下载`);
        }
        saveObjectUrl(URL.createObjectURL(blob), plan.fileName);
        // 说「已开始」而不是「已下载」：`a.click()` 之后浏览器才去读那个 blob，写盘是异步的。
        toast.success(t`已开始下载 ${plan.fileName}`, {
          // 部分跳过要说出来——一个悄悄少了文件的压缩包比一次明确的失败更难发现。
          // `<Plural>` 而不是插值：源 locale 只给 `other`（中文无单复数），en 由译者补
          // `one`，否则英文界面会显示「1 files」。sonner 的 description 收 ReactNode。
          description:
            skipped.length > 0 ? (
              <Plural
                value={skipped.length}
                other="有 # 个文件已不在浏览器存储中，未能打包"
              />
            ) : undefined,
        });
        // **不 await**：下载已经交给浏览器了，标已读是一次独立的 DB 往返（自带 try/catch）。
        // 挂在它上面会让这个 async 帧多活一个往返，而帧里躺着**整包已物化的 zip**——
        // 挂起的帧不会被单独回收，那份字节就跟着多钉一会儿。
        void markOpened(item);
      })
        // 成功与失败都要放行下一次。`run` 自己吞了异常（错误落进 keyed action），
        // 所以这里 `.finally` 恒会跑到——但闸门必须在它之后清，否则失败一次就再也打不了包。
        .finally(() => {
          zipInFlight.current = false;
        });
    },
    [downloadAction.run, markOpened, t],
  );

  /**
   * 整条记录一次取走。逐文件点下去在多文件条目上是纯粹的重复劳动，而收件箱最常见的形态
   * 恰恰是「对方一次发来一整个文件夹」。
   */
  const downloadAll = useCallback(
    (item: InboxItemDetail) => {
      runZipDownload(
        item,
        allDownloadKey(item.id),
        // 全部文件不同处一个顶层目录时（对方一次发来几个散文件）没有「那个目录名」可用，
        // 回落到一句通用的——拿首文件名去命名一个装着五个文件的压缩包只会误导。
        planItemZip(usableInboxFiles(item), t`收到的文件`),
      );
    },
    [runZipDownload, t],
  );

  /**
   * 取回一个目标：文件直接下载，目录（含全部后代）打成 zip。
   *
   * **两种目标在这里分派，而不是让详情面板拆成两个回调**——目录是一个独立目标而不是文件
   * 的循环，这正是 `FileBrowserActions.onDownload` 收 target 的理由。
   *
   * 目录的成员判据在 `planDirectoryZip` 里（按 segment 边界比，`a/b` 不会误匹配 `a/bc`）。
   */
  const downloadTarget = useCallback(
    (item: InboxItemDetail, target: FileBrowserTarget) => {
      if (target.type === "file") {
        const file = item.files.find(
          (candidate) => String(candidate.id) === target.item.sourceId,
        );
        if (file) download(item, file);
        return;
      }
      runZipDownload(
        item,
        directoryDownloadKey(item.id, target.relativePath),
        planDirectoryZip(usableInboxFiles(item), target.relativePath),
      );
    },
    [download, runZipDownload],
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
            <div
              ref={listRef}
              onKeyDown={onListKeyDown}
              className="flex min-h-0 flex-col gap-4 overflow-y-auto"
            >
              {/* 截断要说出来。命中正好等于上限时无法区分「刚好这么多」与「还有更多」，
                  所以文案取保守说法——比静默丢掉让用户以为搜全了要诚实。
                  上限从内核读（`inbox_search_limit()`），前端不自带这个数字。 */}
              {isSearching && searchLimit !== null && hits.length >= searchLimit && (
                <p className="text-[11px] text-muted-foreground">
                  <Trans>
                    只显示最近 {searchLimit} 条匹配，更早的未列出——请把关键词写得更具体。
                  </Trans>
                </p>
              )}

              {/*
                检索态**不分组**：命中已按相关度而非时间排列，套上「今天 / 昨天」会暗示
                一个并不存在的时序。桌面同此。
              */}
              {isSearching ? (
                <ul className="flex flex-col gap-1.5">
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
              ) : (
                groupedRows.map((group) => (
                  <div key={group.key}>
                    {/* 吸顶：滚动时组头压在内容上方，一屏里始终知道自己在看哪一段。 */}
                    <p className="sticky top-0 z-10 bg-card/95 py-1 text-[11px] font-medium text-muted-foreground backdrop-blur">
                      {t(TIME_BUCKET_LABEL[group.key])}
                    </p>
                    <ul className="mt-1 flex flex-col gap-1.5">
                      {group.items.map(({ item, snippet }) => (
                        <InboxListRow
                          key={item.id}
                          item={item}
                          snippet={snippet}
                          selected={item.id === focusedId}
                          onSelect={closeDrawer}
                        />
                      ))}
                    </ul>
                  </div>
                ))
              )}
            </div>
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
            onDownload={downloadTarget}
            onDownloadAll={downloadAll}
          />
        ) : (
          <InboxDetailEmpty openList={openList} hasRows={rows.length > 0} />
        )
      }
    />
  );
}
