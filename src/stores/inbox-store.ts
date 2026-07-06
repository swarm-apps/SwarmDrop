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
}

export const useInboxStore = create<InboxState>()((set) => ({
  items: [],
  detail: null,
  detailForId: null,
  loading: true,
  searching: false,
  searchResults: null,

  async loadItems(includeArchived) {
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
    if (trimmed === "") {
      set({ searchResults: null, searching: false });
      return;
    }
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
}));
