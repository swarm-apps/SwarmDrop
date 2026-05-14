import type {
  MobileTransferOffer as TransferOffer,
  MobileTransferSession as TransferSession,
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
  sessions: Record<string, TransferSession>;
  progress: Record<string, number>;
  lastError: string | null;
}

interface TransferActions {
  pushOffer(offer: TransferOffer): void;
  dismissOffer(id: string): void;
  upsertSession(session: TransferSession): void;
  setProgress(sessionId: string, value: number): void;
  removeSession(sessionId: string): void;
  setError(message: string | null): void;
  reset(): void;
}

export const useTransferStore = create<TransferState & TransferActions>()(
  (set, get) => ({
    offerQueue: [],
    currentOffer: null,
    sessions: {},
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

    upsertSession(session) {
      set((s) => ({
        sessions: { ...s.sessions, [session.sessionId]: session },
      }));
    },

    setProgress(sessionId, value) {
      set((s) => ({ progress: { ...s.progress, [sessionId]: value } }));
    },

    removeSession(sessionId) {
      set((s) => {
        const sessions = { ...s.sessions };
        const progress = { ...s.progress };
        delete sessions[sessionId];
        delete progress[sessionId];
        return { sessions, progress };
      });
    },

    setError(message) {
      set({ lastError: message });
    },

    reset() {
      set({
        offerQueue: [],
        currentOffer: null,
        sessions: {},
        progress: {},
        lastError: null,
      });
    },
  }),
);
