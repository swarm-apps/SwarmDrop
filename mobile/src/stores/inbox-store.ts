import { File } from "expo-file-system";
import type {
  MobileInboxFileEntry,
  MobileInboxItemDetail,
  MobileInboxItemSummary,
  MobileInboxSearchHit,
} from "react-native-swarmdrop-core";
import { create } from "zustand";
import { getMobileCore } from "@/core/mobile-core";
import { errorMessage } from "@/lib/utils";

/** mobile-core 是否导出了服务端 FTS 检索(旧原生无此绑定时回退客户端过滤)。 */
export function supportsServerInboxSearch(): boolean {
  return typeof getMobileCore().searchInbox === "function";
}

const SEARCH_LIMIT = 100;

type InboxAction =
  | "archive"
  | "delete-record"
  | "delete-content"
  | "mark-missing"
  | "repair";

interface DeleteInboxOptions {
  deleteLocalFiles?: boolean;
}

interface InboxState {
  loading: boolean;
  detailLoading: boolean;
  items: MobileInboxItemSummary[];
  selectedDetail: MobileInboxItemDetail | null;
  selectedItemId: string | null;
  action: InboxAction | null;
  lastError: string | null;
  lastRefreshedAt: number | null;
  /** 服务端 FTS 检索命中;null = 尚未检索 / 不支持(回退客户端过滤)。 */
  searchResults: MobileInboxSearchHit[] | null;
  searching: boolean;
}

interface InboxActions {
  refresh(): Promise<void>;
  repairMissingItems(): Promise<number>;
  loadDetail(itemId: string): Promise<MobileInboxItemDetail | null>;
  clearDetail(): void;
  markOpened(itemId: string): Promise<void>;
  archiveItem(itemId: string, archived: boolean): Promise<void>;
  deleteItem(itemId: string, options?: DeleteInboxOptions): Promise<void>;
  markFileMissing(
    itemId: string,
    fileId: number,
    missing?: boolean,
  ): Promise<void>;
  /** 服务端 FTS 检索(标题 / 来源 / 文件名+路径 / 文档正文)。需 mobile-core 绑定支持。 */
  runSearch(query: string, includeArchived: boolean): Promise<void>;
  clearSearch(): void;
  reset(): void;
}

export type InboxStore = InboxState & InboxActions;
export type InboxPreviewItem = MobileInboxItemSummary;
export type InboxDetailItem = MobileInboxItemDetail;
export type InboxFileEntry = MobileInboxFileEntry;
export type InboxSearchHit = MobileInboxSearchHit;

// 并发 refresh 的单调序号：多触发源下迟到的旧响应不得覆盖新结果。
let refreshSeq = 0;
// 并发 search 的单调序号：防抖 / 快速输入下迟到的旧检索结果不得覆盖新结果。
let searchSeq = 0;

export const useInboxStore = create<InboxStore>()((set, get) => ({
  loading: false,
  detailLoading: false,
  items: [],
  selectedDetail: null,
  selectedItemId: null,
  action: null,
  lastError: null,
  lastRefreshedAt: null,
  searchResults: null,
  searching: false,

  async runSearch(query, includeArchived) {
    const trimmed = query.trim();
    if (!supportsServerInboxSearch() || trimmed.length === 0) {
      // 不支持服务端检索或查询为空：清掉结果,交回客户端过滤路径。
      searchSeq++;
      set({ searchResults: null, searching: false });
      return;
    }
    const seq = ++searchSeq;
    set({ searching: true, lastError: null });
    try {
      const hits = await getMobileCore().searchInbox(
        trimmed,
        SEARCH_LIMIT,
        includeArchived,
      );
      if (seq !== searchSeq) return; // 丢弃过期检索响应
      set({ searchResults: hits });
    } catch (err) {
      if (seq !== searchSeq) return;
      set({ searchResults: null, lastError: errorMessage(err) });
      console.warn("[inbox-store] search failed:", errorMessage(err));
    } finally {
      if (seq === searchSeq) set({ searching: false });
    }
  },

  clearSearch() {
    searchSeq++;
    set({ searchResults: null, searching: false });
  },

  async refresh() {
    const seq = ++refreshSeq;
    set({ loading: true, lastError: null });
    try {
      const items = await getMobileCore().listInboxItems(true);
      // 丢弃过期响应：focus / 归档 / 删除 / 事件等并发触发时，旧响应不得覆盖新结果。
      if (seq !== refreshSeq) return;
      set({ items, lastRefreshedAt: Date.now() });
    } catch (err) {
      if (seq !== refreshSeq) return;
      set({ lastError: errorMessage(err) });
      console.warn("[inbox-store] refresh failed:", errorMessage(err));
    } finally {
      if (seq === refreshSeq) set({ loading: false });
    }
  },

  async repairMissingItems() {
    set({ action: "repair", lastError: null });
    try {
      const repaired = await getMobileCore().repairMissingInboxItems();
      await get().refresh();
      return repaired.length;
    } catch (err) {
      set({ lastError: errorMessage(err) });
      throw err;
    } finally {
      set({ action: null });
    }
  },

  async loadDetail(itemId) {
    set({ detailLoading: true, selectedItemId: itemId, lastError: null });
    try {
      const detail = await getMobileCore().getInboxItem(itemId);
      set({ selectedDetail: detail ?? null });
      return detail ?? null;
    } catch (err) {
      set({ selectedDetail: null, lastError: errorMessage(err) });
      console.warn("[inbox-store] loadDetail failed:", errorMessage(err));
      return null;
    } finally {
      set({ detailLoading: false });
    }
  },

  clearDetail() {
    set({ selectedDetail: null, selectedItemId: null, detailLoading: false });
  },

  async markOpened(itemId) {
    try {
      await getMobileCore().markInboxItemOpened(itemId);
    } catch (err) {
      console.warn("[inbox-store] markOpened failed:", errorMessage(err));
    }
  },

  async archiveItem(itemId, archived) {
    set({ action: "archive", lastError: null });
    try {
      await getMobileCore().archiveInboxItem(itemId, archived);
      set((state) => ({
        items: state.items.map((item) =>
          item.id === itemId
            ? {
                ...item,
                archivedAt: archived ? BigInt(Date.now()) : undefined,
              }
            : item,
        ),
        selectedDetail:
          state.selectedDetail?.item.id === itemId
            ? {
                ...state.selectedDetail,
                item: {
                  ...state.selectedDetail.item,
                  archivedAt: archived ? BigInt(Date.now()) : undefined,
                },
              }
            : state.selectedDetail,
      }));
    } catch (err) {
      set({ lastError: errorMessage(err) });
      throw err;
    } finally {
      set({ action: null });
    }
  },

  async deleteItem(itemId, options = {}) {
    set({
      action: options.deleteLocalFiles ? "delete-content" : "delete-record",
      lastError: null,
    });
    try {
      if (options.deleteLocalFiles) {
        const detail =
          get().selectedDetail?.item.id === itemId
            ? get().selectedDetail
            : await getMobileCore().getInboxItem(itemId);
        if (detail) {
          await deleteLocalFiles(detail.files);
        }
      }
      await getMobileCore().deleteInboxItemRecord(itemId);
      set((state) => ({
        items: state.items.filter((item) => item.id !== itemId),
        selectedDetail:
          state.selectedDetail?.item.id === itemId
            ? null
            : state.selectedDetail,
        selectedItemId:
          state.selectedItemId === itemId ? null : state.selectedItemId,
      }));
    } catch (err) {
      set({ lastError: errorMessage(err) });
      throw err;
    } finally {
      set({ action: null });
    }
  },

  async markFileMissing(itemId, fileId, missing = true) {
    set({ action: "mark-missing", lastError: null });
    try {
      await getMobileCore().markInboxFileMissing(itemId, fileId, missing);
      set((state) => ({
        items: state.items.map((item) =>
          item.id === itemId ? { ...item, missing } : item,
        ),
        selectedDetail:
          state.selectedDetail?.item.id === itemId
            ? {
                ...state.selectedDetail,
                item: { ...state.selectedDetail.item, missing },
                files: state.selectedDetail.files.map((file) =>
                  file.id === fileId ? { ...file, missing } : file,
                ),
              }
            : state.selectedDetail,
      }));
    } catch (err) {
      set({ lastError: errorMessage(err) });
      throw err;
    } finally {
      set({ action: null });
    }
  },

  reset() {
    set({
      loading: false,
      detailLoading: false,
      items: [],
      selectedDetail: null,
      selectedItemId: null,
      action: null,
      lastError: null,
      lastRefreshedAt: null,
      searchResults: null,
      searching: false,
    });
  },
}));

async function deleteLocalFiles(files: MobileInboxFileEntry[]): Promise<void> {
  const failures: string[] = [];
  for (const entry of files) {
    try {
      const file = new File(entry.localPath);
      if (file.exists) {
        file.delete();
      }
    } catch (err) {
      failures.push(`${entry.name}: ${errorMessage(err)}`);
    }
  }
  if (failures.length > 0) {
    throw new Error(failures.join("\n"));
  }
}
