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
}

export const useInboxStore = create<InboxState>()((set, get) => ({
  items: [],
  selectedId: null,
  detail: null,
  loading: true,
  showArchived: false,

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
  },
}));
