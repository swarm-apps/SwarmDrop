/**
 * 收件箱 store
 *
 * 封装收件箱列表 + 选中项 + 详情的加载与选择逻辑。选中项的「保留/改选」逻辑放在
 * loadItems action 里，基于上一份 state.selectedId 计算——不再依赖组件里的
 * selectedIdRef（useRef）去规避 setState 闭包陈旧。
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
  selectedId: string | null;
  detail: InboxItemDetail | null;
  loading: boolean;
  showArchived: boolean;

  /** 拉取列表，返回刷新后真正生效的 selectedId（保留旧选中或改选到第一条）。 */
  loadItems: () => Promise<string | null>;
  loadDetail: (itemId: string | null) => Promise<void>;
  selectItem: (id: string) => void;
  setShowArchived: (value: boolean) => void;
  /** 执行一个操作后刷新列表与详情（用刷新后生效的 selectedId 重取详情）。 */
  runAndRefresh: (
    action: () => Promise<unknown>,
    success?: string,
  ) => Promise<void>;

  // —— 搜索 ——
  /** 当前搜索词。 */
  query: string;
  /** 是否正在请求搜索。 */
  searching: boolean;
  /** 搜索命中；`null` 表示未处于搜索态（展示原列表）。 */
  searchResults: InboxSearchHit[] | null;
  /** 设置搜索词；清空时退出搜索态。实际请求由组件防抖后调用 [`runSearch`]。 */
  setQuery: (q: string) => void;
  /** 按当前搜索词执行检索（尊重归档过滤）。 */
  runSearch: () => Promise<void>;
}

export const useInboxStore = create<InboxState>()((set, get) => ({
  items: [],
  selectedId: null,
  detail: null,
  loading: true,
  showArchived: false,
  query: "",
  searching: false,
  searchResults: null,

  async loadItems() {
    set({ loading: true });
    try {
      const { showArchived, selectedId } = get();
      const next = await commands.listInboxItems(showArchived);
      // 基于上一份 state.selectedId 计算「保留还是改选」，避免用陈旧 id 去 loadDetail。
      const resolved =
        selectedId && next.some((item) => item.id === selectedId)
          ? selectedId
          : (next[0]?.id ?? null);
      set({ items: next, selectedId: resolved });
      return resolved;
    } catch (err) {
      toast.error(getErrorMessage(err));
      return null;
    } finally {
      set({ loading: false });
    }
  },

  async loadDetail(itemId) {
    if (!itemId) {
      set({ detail: null });
      return;
    }
    try {
      set({ detail: await commands.getInboxItemDetail(itemId) });
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  },

  selectItem(id) {
    set({ selectedId: id });
  },

  setShowArchived(value) {
    set({ showArchived: value });
  },

  setQuery(q) {
    set({ query: q });
    // 清空即退出搜索态，立即恢复原列表（不必等防抖）。
    if (q.trim() === "") {
      set({ searchResults: null, searching: false });
    }
  },

  async runSearch() {
    const { query, showArchived } = get();
    const trimmed = query.trim();
    if (trimmed === "") {
      set({ searchResults: null, searching: false });
      return;
    }
    set({ searching: true });
    try {
      const hits = await commands.searchInbox(trimmed, null, showArchived);
      set({ searchResults: hits });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      set({ searching: false });
    }
  },

  async runAndRefresh(action, success) {
    try {
      await action();
      if (success) toast.success(success);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
    // 用刷新后真正生效的 selectedId 重取详情，避免取到刚被删除/归档隐藏的失效条目。
    const resolved = await get().loadItems();
    await get().loadDetail(resolved);
    // 搜索态下，删除/归档后同步重搜，使结果列表不残留失效条目。
    if (get().searchResults !== null) {
      await get().runSearch();
    }
  },
}));
