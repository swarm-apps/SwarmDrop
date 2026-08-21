/**
 * 收件箱 store
 *
 * 封装收件箱列表、详情和搜索结果缓存。当前选中的条目、搜索词和归档过滤属于路由
 * search params，避免跨页面通过 store 预设导航目标。
 */

import { create } from "zustand";
import { toast } from "sonner";
import {
  commands,
  events,
  type InboxItemDetail,
  type InboxItemSummary,
  type InboxSearchHit,
} from "@/lib/bindings";
import { getErrorMessage } from "@/lib/errors";

interface InboxState {
  items: InboxItemSummary[];
  detail: InboxItemDetail | null;
  /** detail 当前对应的条目 id（含“已加载但为空/失败”的判定）。用于区分“加载中”与“加载完成但无详情”，避免详情失败时无限骨架。 */
  detailForId: string | null;
  loading: boolean;

  /** 拉取列表；当前是否包含归档项由路由层传入。 */
  loadItems: (includeArchived: boolean) => Promise<InboxItemSummary[]>;
  loadDetail: (itemId: string | null) => Promise<void>;

  // —— 搜索 ——
  /** 是否正在请求搜索。 */
  searching: boolean;
  /** 搜索命中；`null` 表示未处于搜索态（展示原列表）。 */
  searchResults: InboxSearchHit[] | null;
  /** 按路由搜索词执行检索（尊重归档过滤）。 */
  runSearch: (query: string, includeArchived: boolean) => Promise<void>;

  /**
   * 领域事件到达后的重取。
   *
   * 与 `loadItems` 的两处差别都是刻意的：
   *
   * 1. **不翻 `loading`**。这次重取不是用户发起的，翻它会让列表在每条到达的条目上
   *    闪一次骨架屏——用户正在看的东西被一个他没做的动作打断。
   * 2. **搜索态也要跟着重取**。搜索命中是**另一份**结果集，UI 在搜索态下渲染的是它
   *    而不是 `items`；只刷列表的话，屏幕上那份命中里会留着已经被删掉的条目，
   *    点进去是一个不存在的详情。
   */
  refreshFromEvent: () => Promise<void>;
}

/**
 * 最后一次拉取用的归档过滤。
 *
 * 放模块级而不是 store 里：它不是 UI 状态，没有任何组件读它，进 store 只会让每次拉取
 * 都广播一轮无人关心的变化。事件到达时用它按**当前视图**重新拉取——归档视图下收到新
 * 条目，刷新的也该是归档视图。
 */
let lastIncludeArchived = false;

/**
 * 最后一次执行的搜索词；`null` = 不在搜索态。
 *
 * 与 `lastIncludeArchived` 同一条理由（不是 UI 状态，没有组件读它），存在的目的也一样：
 * 事件到达时要能按**当前视图**重取，而搜索词住在路由的 search params 里，store 拿不到。
 */
let lastQuery: string | null = null;

export const useInboxStore = create<InboxState>()((set) => ({
  items: [],
  detail: null,
  detailForId: null,
  loading: true,
  searching: false,
  searchResults: null,

  async loadItems(includeArchived) {
    lastIncludeArchived = includeArchived;
    set({ loading: true });
    try {
      const next = await commands.listInboxItems(includeArchived);
      set({ items: next });
      return next;
    } catch (err) {
      toast.error(getErrorMessage(err));
      return [];
    } finally {
      set({ loading: false });
    }
  },

  async loadDetail(itemId) {
    if (!itemId) {
      set({ detail: null, detailForId: null });
      return;
    }
    try {
      const next = await commands.getInboxItemDetail(itemId);
      set({ detail: next, detailForId: itemId });
    } catch (err) {
      // 加载失败：清掉旧详情并标记该 id 已“尝试完成”，让 UI 走失败态而非无限骨架。
      set({ detail: null, detailForId: itemId });
      toast.error(getErrorMessage(err));
    }
  },

  async runSearch(query, includeArchived) {
    const trimmed = query.trim();
    lastIncludeArchived = includeArchived;
    if (trimmed === "") {
      lastQuery = null;
      set({ searchResults: null, searching: false });
      return;
    }
    lastQuery = trimmed;
    set({ searching: true });
    try {
      const hits = await commands.searchInbox(trimmed, null, includeArchived);
      set({ searchResults: hits });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      set({ searching: false });
    }
  },

  async refreshFromEvent() {
    // 两份结果集各自重取。**不翻 `loading` / `searching`**：这次重取不是用户发起的，
    // 翻它们会让屏幕在每条到达的条目上闪一次骨架屏。
    const [items, hits] = await Promise.all([
      commands.listInboxItems(lastIncludeArchived).catch(() => null),
      lastQuery === null
        ? Promise.resolve(null)
        : commands.searchInbox(lastQuery, null, lastIncludeArchived).catch(() => null),
    ]);
    // 失败一律**保持原样**，不弹 toast：用户没做这件事，为一次后台重取报错只会让人
    // 以为出了故障；而下一条事件（或他自己切换视图）就会再试一次。
    if (items) set({ items });
    if (hits) set({ searchResults: hits });
  },
}));

let unlistenFns: Array<() => void> = [];

/**
 * 注册与注销的串行队列。
 *
 * ⚠️ **不能只靠「先 await cleanup 再赋值」**：React StrictMode 的
 * mount → unmount → mount 会让两次 setup 与一次 cleanup 交叠。第一次 cleanup 跑到时
 * 第一批 `listen()` 还没 resolve，`unlistenFns` 仍是空数组——它一个都注销不掉，
 * 而随后两次 setup 先后写同一个模块级变量，**先前那三个句柄就此永远丢失**。
 * 表现是每条收件箱事件触发两次重取（两个并发 IPC，返回顺序还不确定），且卸载之后
 * 监听器仍在。
 *
 * 串成一条链之后，注册与注销严格按调用顺序发生，不可能交叠。
 *
 * ⚠️ `transfer-store.ts` 的 `setupTransferListeners` 是同一个形状、同一个隐患，
 * 本次没有一并改（它不在这次改动的范围内），但下一个碰它的人应当照这里改。
 */
let queue: Promise<void> = Promise.resolve();

/**
 * 订阅收件箱领域事件。
 *
 * ## 为什么不从 `transferComplete` 推导
 *
 * 那需要依赖「先建条目、再发完成事件」这条只以行内注释存在的顺序，而且要自己判
 * `direction === "receive"`（移动端此前那份就漏了，于是**发送**完成也白刷一次）。
 * 后端现在发一等的收件箱事件，三端订阅同一个信号（spec: `inbox-domain-events`）。
 *
 * ## 为什么是「重新拉取」而不是把事件载荷插进列表
 *
 * 事件载荷**刻意很窄**——它不含标题，因为文本条目的标题就是正文前 160 字节，
 * 而事件会流经日志。要展示就得有完整的 `InboxItemSummary`，所以拿到信号后重新拉一次。
 * 收件箱的变更频率是「人发过来一次」的量级，不是热路径。
 */
export function setupInboxListeners() {
  queue = queue.then(async () => {
    await teardown();

    const refresh = () => {
      void useInboxStore.getState().refreshFromEvent();
    };

    unlistenFns = await Promise.all([
      events.inboxItemAdded.listen(refresh),
      events.inboxItemArchived.listen(refresh),
      events.inboxItemRemoved.listen((event) => {
        // 打开着的正是被删掉的那条 ⇒ 就地清空详情。
        // **不去重取它**：那次请求必然失败，而失败路径会弹一个错误 toast——
        // 用户根本没做这件事（可能是 MCP 或另一个窗口删的），弹给他只会像故障。
        const state = useInboxStore.getState();
        if (state.detailForId === event.payload.itemId) {
          void state.loadDetail(null);
        }
        refresh();
      }),
    ]);
  });
  return queue;
}

export function cleanupInboxListeners() {
  queue = queue.then(teardown);
  return queue;
}

async function teardown() {
  for (const unlisten of unlistenFns) unlisten();
  unlistenFns = [];
}
