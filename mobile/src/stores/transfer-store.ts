import type {
  MobileTransferOffer as TransferOffer,
  MobileTransferProgress,
} from "react-native-swarmdrop-core";
import { create } from "zustand";

interface TransferOfferQueueItem {
  id: string;
  offer: TransferOffer;
  receivedAt: number;
}

interface TransferState {
  offerQueue: TransferOfferQueueItem[];
  currentOffer: TransferOfferQueueItem | null;
  /** 活跃 session id 集合（accept_receive / send_prepared 后注册，complete/fail 时移除） */
  activeSessionIds: Set<string>;
  /** 各 session 最新进度快照（含 speed/eta，由 EventBus 直接写入） */
  progress: Record<string, MobileTransferProgress>;
  lastError: string | null;
}

interface TransferActions {
  pushOffer(offer: TransferOffer): void;
  dismissOffer(id: string): void;
  registerSession(sessionId: string): void;
  setProgress(sessionId: string, snapshot: MobileTransferProgress): void;
  removeSession(sessionId: string): void;
  setError(message: string | null): void;
  reset(): void;
}

export const useTransferStore = create<TransferState & TransferActions>()(
  (set, get) => ({
    offerQueue: [],
    currentOffer: null,
    activeSessionIds: new Set(),
    progress: {},
    lastError: null,

    pushOffer(offer) {
      const item: TransferOfferQueueItem = {
        id: offer.sessionId,
        offer,
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

    registerSession(sessionId) {
      set((s) => ({
        activeSessionIds: new Set(s.activeSessionIds).add(sessionId),
      }));
    },

    setProgress(sessionId, snapshot) {
      set((s) => ({ progress: { ...s.progress, [sessionId]: snapshot } }));
    },

    removeSession(sessionId) {
      set((s) => {
        const activeSessionIds = new Set(s.activeSessionIds);
        activeSessionIds.delete(sessionId);
        const progress = { ...s.progress };
        delete progress[sessionId];
        return { activeSessionIds, progress };
      });
    },

    setError(message) {
      set({ lastError: message });
    },

    reset() {
      set({
        offerQueue: [],
        currentOffer: null,
        activeSessionIds: new Set(),
        progress: {},
        lastError: null,
      });
    },
  }),
);
