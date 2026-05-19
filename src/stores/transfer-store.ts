import { create } from "zustand";
import {
  commands,
  events,
  type TransferOfferEvent,
  type TransferProgressEvent,
  type TransferCompleteEvent,
  type TransferFailedEvent,
  type TransferHistoryItem,
} from "@/lib/bindings";
import type { TransferSession } from "@/lib/types";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";

interface TransferState {
  sessions: Record<string, TransferSession>;
  dbHistory: TransferHistoryItem[];
  pendingOffers: TransferOfferEvent[];

  addSession: (session: TransferSession) => void;
  updateProgress: (event: TransferProgressEvent) => void;
  completeSession: (event: TransferCompleteEvent) => void;
  failSession: (event: TransferFailedEvent) => void;
  cancelSession: (sessionId: string) => void;
  removeSession: (sessionId: string) => void;
  pushOffer: (offer: TransferOfferEvent) => void;
  shiftOffer: () => TransferOfferEvent | undefined;
  getActiveCount: () => number;
  loadHistory: () => Promise<void>;
}

let unlistenFns: Array<() => void> = [];

export async function setupTransferListeners() {
  await cleanupTransferListeners();

  await useTransferStore.getState().loadHistory();

  const fns = await Promise.all([
    events.transferOffer.listen((event) => {
      useTransferStore.getState().pushOffer(event.payload);
    }),

    events.transferProgress.listen((event) => {
      useTransferStore.getState().updateProgress(event.payload);
    }),

    events.transferComplete.listen((event) => {
      useTransferStore.getState().completeSession(event.payload);
    }),

    events.transferFailed.listen((event) => {
      useTransferStore.getState().failSession(event.payload);
    }),

    events.transferPaused.listen((event) => {
      // 对端暂停传输：移除活跃 session，刷新历史（DB 中已标记为 paused）
      removeAndRefresh(event.payload.sessionId);
      toast.info(t`对方已暂停传输`);
    }),

    events.transferResumed.listen((event) => {
      // 对端（发送方）发起恢复传输：添加到活跃 session，刷新历史
      const { sessionId, direction, peerId, peerName, files, totalSize } =
        event.payload;
      // resume 路径只对已建立的 session 触发，方向必然是 send/receive；
      // 收到 "unknown" 视为后端 bug，跳过避免污染 store。
      if (direction === "unknown") {
        console.warn(`[transferResumed] ignoring unknown direction: ${sessionId}`);
        return;
      }
      useTransferStore.getState().addSession({
        sessionId,
        direction,
        peerId,
        deviceName: peerName,
        files,
        totalSize,
        status: "transferring",
        progress: null,
        error: null,
        startedAt: Date.now(),
        completedAt: null,
      });
      useTransferStore.getState().loadHistory();
    }),

    events.transferAccepted.listen((event) => {
      const { sessionId } = event.payload;
      useTransferStore.setState((state) => {
        const session = state.sessions[sessionId];
        if (!session) return state;
        return {
          sessions: {
            ...state.sessions,
            [sessionId]: { ...session, status: "transferring" },
          },
        };
      });
    }),

    events.transferRejected.listen((event) => {
      const { sessionId, reason } = event.payload;
      useTransferStore.setState((state) => {
        const { [sessionId]: _, ...rest } = state.sessions;
        return { sessions: rest };
      });
      if (reason?.type === "not_paired") {
        toast.error(t`设备已取消配对`);
      } else {
        toast.error(t`对方拒绝了请求`);
      }
    }),

    events.transferDbError.listen((event) => {
      const { message } = event.payload;
      toast.error(message);
    }),
  ]);

  unlistenFns = fns;
}

export async function cleanupTransferListeners() {
  for (const unlisten of unlistenFns) {
    unlisten();
  }
  unlistenFns = [];
}

/** 从活跃 sessions 中移除指定 session，并刷新 DB 历史 */
function removeAndRefresh(sessionId: string) {
  // 先刷新历史，再移除活跃 session，避免出现空状态闪烁
  useTransferStore
    .getState()
    .loadHistory()
    .finally(() => {
      useTransferStore.setState((state) => {
        const { [sessionId]: _, ...rest } = state.sessions;
        return { sessions: rest };
      });
    });
}

export const useTransferStore = create<TransferState>()((set, get) => ({
  sessions: {},
  dbHistory: [],
  pendingOffers: [],

  addSession(session) {
    set((state) => ({
      sessions: { ...state.sessions, [session.sessionId]: session },
    }));
  },

  updateProgress(event) {
    set((state) => {
      const session = state.sessions[event.sessionId];
      if (!session) return state;
      return {
        sessions: {
          ...state.sessions,
          [event.sessionId]: {
            ...session,
            status: "transferring",
            progress: event,
          },
        },
      };
    });
  },

  completeSession(event) {
    removeAndRefresh(event.sessionId);
  },

  failSession(event) {
    removeAndRefresh(event.sessionId);
  },

  cancelSession(sessionId) {
    removeAndRefresh(sessionId);
  },

  removeSession(sessionId) {
    set((state) => {
      const { [sessionId]: _, ...rest } = state.sessions;
      return { sessions: rest };
    });
  },

  pushOffer(offer) {
    set((state) => ({
      pendingOffers: [...state.pendingOffers, offer],
    }));
  },

  shiftOffer() {
    const { pendingOffers } = get();
    if (pendingOffers.length === 0) return undefined;
    const [first, ...rest] = pendingOffers;
    set({ pendingOffers: rest });
    return first;
  },

  getActiveCount() {
    return Object.keys(get().sessions).length;
  },

  async loadHistory() {
    try {
      const items = await commands.getTransferHistory(null);
      set({ dbHistory: items });
    } catch (e) {
      console.error("加载传输历史失败:", e);
    }
  },
}));
