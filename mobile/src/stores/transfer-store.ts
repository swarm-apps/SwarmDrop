import { t } from "@lingui/core/macro";
import type {
  MobileSessionStatus,
  MobileTransferFile,
  MobileTransferHistoryItem,
  MobileTransferProgress,
  MobileTransferResumed,
  MobileTransferOffer as NativeTransferOffer,
} from "react-native-swarmdrop-core";
import { create } from "zustand";

import { getMobileCore } from "@/core/mobile-core";
import type {
  RegisterSessionInput,
  TransferOfferQueueItem,
  TransferSession,
} from "@/core/transfer-types";
import { errorMessage } from "@/lib/utils";

interface TransferState {
  /** 入站 offer 队列（接收方等用户响应；首条会被 transfer-offer-host 弹窗显示） */
  offerQueue: TransferOfferQueueItem[];
  currentOffer: TransferOfferQueueItem | null;

  /** 活跃会话（内存），key 为 sessionId */
  sessions: Record<string, TransferSession>;

  /** SQLite 中持久化的历史快照（每次 loadHistory 后被替换） */
  dbHistory: MobileTransferHistoryItem[];

  /** 最近一次错误，主要给 toast 用 */
  lastError: string | null;
}

interface TransferActions {
  /** 入站 offer 进队 */
  pushOffer(offer: NativeTransferOffer): void;
  /** 用户响应（接受/拒绝/超时）后从队列移除 */
  dismissOffer(id: string): void;

  /** 注册新活跃会话（send/accept/resume 路径调用，提供完整 metadata） */
  addSession(input: RegisterSessionInput): void;

  /** TransferProgress 事件 → 更新 sessions[id].progress */
  updateProgress(snapshot: MobileTransferProgress): void;

  /** TransferAccepted 事件 → sessions[id].status = transferring */
  markAccepted(sessionId: string): void;

  /** TransferRejected 事件 → 从 sessions 删除（不进历史，因为没建立过 DB session） */
  markRejected(sessionId: string): void;

  /** TransferResumed 事件 → addSession({ initialStatus: "transferring" }) + loadHistory */
  resumedSession(event: MobileTransferResumed): void;

  /** 完成 / 失败 / 暂停：先 await loadHistory() 再从 sessions 移除（避免 UI 空窗） */
  removeAndRefresh(sessionId: string): Promise<void>;

  /**
   * 发送一批文件：prepareSend → sendPrepared → addSession，返回新 sessionId。
   * 调用方（select-device / [sessionId] 重新发送）只负责构造 files 与跳路由。
   */
  startSend(input: {
    files: MobileTransferFile[];
    peerId: string;
    peerName: string;
  }): Promise<string>;

  /** 调用 native list_transfer_history 并替换 dbHistory */
  loadHistory(): Promise<void>;

  /** 调用 native clear_transfer_history + loadHistory */
  clearAllHistory(): Promise<void>;

  /** 调用 native delete_transfer_session + loadHistory */
  deleteHistoryItem(sessionId: string): Promise<void>;

  /** 调用 native resume_transfer + addSession + loadHistory；返回新 sessionId */
  resumeHistoryItem(sessionId: string): Promise<string>;

  setError(message: string | null): void;
  reset(): void;
}

export const useTransferStore = create<TransferState & TransferActions>()(
  (set, get) => ({
    offerQueue: [],
    currentOffer: null,
    sessions: {},
    dbHistory: [],
    lastError: null,

    pushOffer(offer) {
      const item: TransferOfferQueueItem = {
        id: offer.sessionId,
        offer: {
          sessionId: offer.sessionId,
          peerId: offer.peerId,
          deviceName: offer.deviceName,
          totalSize: offer.totalSize,
          files: offer.files,
        },
        receivedAt: Date.now(),
      };
      const { currentOffer } = get();
      if (currentOffer === null) {
        set({ currentOffer: item });
      } else {
        set((s) => ({ offerQueue: [...s.offerQueue, item] }));
      }
    },

    dismissOffer(id) {
      const { currentOffer, offerQueue } = get();
      if (currentOffer?.id === id) {
        const [next, ...rest] = offerQueue;
        set({ currentOffer: next ?? null, offerQueue: rest });
      } else {
        set({ offerQueue: offerQueue.filter((q) => q.id !== id) });
      }
    },

    addSession(input) {
      const session: TransferSession = {
        sessionId: input.sessionId,
        direction: input.direction,
        peerId: input.peerId,
        peerName: input.peerName,
        files: input.files,
        totalSize: input.totalSize,
        status:
          input.initialStatus ??
          (input.direction === "send" ? "waiting_accept" : "transferring"),
        progress: null,
        error: null,
        startedAt: Date.now(),
        completedAt: null,
      };
      set((s) => ({
        sessions: { ...s.sessions, [input.sessionId]: session },
      }));
    },

    updateProgress(snapshot) {
      set((s) => {
        const existing = s.sessions[snapshot.sessionId];
        if (!existing) return s;
        return {
          sessions: {
            ...s.sessions,
            [snapshot.sessionId]: {
              ...existing,
              status: "transferring",
              progress: snapshot,
            },
          },
        };
      });
    },

    markAccepted(sessionId) {
      set((s) => {
        const existing = s.sessions[sessionId];
        if (!existing) return s;
        return {
          sessions: {
            ...s.sessions,
            [sessionId]: { ...existing, status: "transferring" },
          },
        };
      });
    },

    markRejected(sessionId) {
      set((s) => {
        if (!s.sessions[sessionId]) return s;
        const { [sessionId]: _removed, ...rest } = s.sessions;
        return { sessions: rest };
      });
    },

    resumedSession(event) {
      if (event.direction !== "send" && event.direction !== "receive") {
        console.warn(
          "[transfer-store] resumedSession: unknown direction",
          event.direction,
        );
        return;
      }
      get().addSession({
        sessionId: event.sessionId,
        direction: event.direction,
        peerId: event.peerId,
        peerName: event.peerName,
        files: event.files.map((f) => ({
          fileId: f.fileId,
          name: f.name,
          relativePath: f.relativePath,
          size: f.size,
          isDirectory: f.isDirectory,
        })),
        totalSize: event.totalSize,
        initialStatus: "transferring",
      });
      void get().loadHistory();
    },

    async removeAndRefresh(sessionId) {
      // 先刷新历史再从活跃集合删除：UI 会先看到历史 section 多一条，
      // 再看到活跃 section 少一条，避免 1 秒"消失"的空窗。
      try {
        await get().loadHistory();
      } finally {
        set((s) => {
          if (!s.sessions[sessionId]) return s;
          const { [sessionId]: _removed, ...rest } = s.sessions;
          return { sessions: rest };
        });
      }
    },

    async startSend(input) {
      const prepared = await getMobileCore().prepareSend(input.files);
      const result = await getMobileCore().sendPrepared(
        prepared.preparedId,
        input.peerId,
        input.peerName,
        // sendPrepared 的 fileIds 是子集筛选；当前 UI 没有子集 UI，必须传全量
        // 否则被 core 当作"未选任何文件"拒绝（见 send.rs）。
        prepared.files.map((f) => f.fileId),
      );
      get().addSession({
        sessionId: result.sessionId,
        direction: "send",
        peerId: input.peerId,
        peerName: input.peerName,
        files: prepared.files.map((f) => ({
          fileId: f.fileId,
          name: f.name,
          relativePath: f.relativePath,
          size: f.size,
          isDirectory: false,
        })),
        totalSize: prepared.totalSize,
      });
      return result.sessionId;
    },

    async loadHistory() {
      try {
        const items = await getMobileCore().listTransferHistory(undefined);
        set({ dbHistory: items });
      } catch (err) {
        console.warn("[transfer-store] loadHistory failed:", errorMessage(err));
        set({ lastError: t`加载传输历史失败` });
      }
    },

    async clearAllHistory() {
      try {
        await getMobileCore().clearTransferHistory();
      } catch (err) {
        set({ lastError: errorMessage(err) });
      } finally {
        await get().loadHistory();
      }
    },

    async deleteHistoryItem(sessionId) {
      try {
        await getMobileCore().deleteTransferSession(sessionId);
      } catch (err) {
        set({ lastError: errorMessage(err) });
      } finally {
        await get().loadHistory();
      }
    },

    async resumeHistoryItem(sessionId) {
      const result = await getMobileCore().resumeTransfer(sessionId);
      if (result.direction !== "send" && result.direction !== "receive") {
        throw new Error(
          `resume_transfer returned invalid direction: ${result.direction}`,
        );
      }
      get().addSession({
        sessionId: result.sessionId,
        direction: result.direction,
        peerId: result.peerId,
        peerName: result.peerName,
        files: result.files.map((f) => ({
          fileId: f.fileId,
          name: f.name,
          relativePath: f.relativePath,
          size: f.size,
          isDirectory: f.isDirectory,
        })),
        totalSize: result.totalSize,
        // resume 路径 native 不会再发 Accepted 事件，直接进 transferring 即可。
        initialStatus: "transferring",
      });
      await get().loadHistory();
      return result.sessionId;
    },

    setError(message) {
      set({ lastError: message });
    },

    reset() {
      set({
        offerQueue: [],
        currentOffer: null,
        sessions: {},
        dbHistory: [],
        lastError: null,
      });
    },
  }),
);

/* ─────────── selectors（派生数据，方便组件订阅） ─────────── */

/** 活跃 session id 列表，按 startedAt 降序 */
export function selectActiveSessionIds(state: TransferState): string[] {
  return Object.values(state.sessions)
    .sort((a, b) => b.startedAt - a.startedAt)
    .map((s) => s.sessionId);
}

/** dbHistory 已经由 native 端按 startedAt 降序返回 */
export function selectHistoryByStatus(
  state: TransferState,
  status: MobileSessionStatus | "all",
): MobileTransferHistoryItem[] {
  if (status === "all") return state.dbHistory;
  return state.dbHistory.filter((item) => item.status === status);
}
