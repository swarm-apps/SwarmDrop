import { create } from "zustand";
import {
  commands,
  events,
  type TransferCompleteEvent,
  type TransferFailedEvent,
  type TransferOfferEvent,
  type TransferProgressEvent,
  type TransferProjection,
} from "@/lib/bindings";
import {
  applyProgressToProjection,
  isProjectionActive,
} from "@/lib/transfer-projection";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";

interface TransferState {
  projections: Record<string, TransferProjection>;
  progressBySession: Record<string, TransferProgressEvent>;
  pendingOffers: TransferOfferEvent[];

  applyProjection: (projection: TransferProjection) => void;
  updateProgress: (event: TransferProgressEvent) => void;
  completeSession: (event: TransferCompleteEvent) => void;
  failSession: (event: TransferFailedEvent) => void;
  cancelSession: (sessionId: string) => void;
  removeSession: (sessionId: string) => void;
  pushOffer: (offer: TransferOfferEvent) => void;
  shiftOffer: () => TransferOfferEvent | undefined;
  getActiveCount: () => number;
  loadProjections: () => Promise<void>;
}

let unlistenFns: Array<() => void> = [];

export async function setupTransferListeners() {
  await cleanupTransferListeners();

  await useTransferStore.getState().loadProjections();

  const fns = await Promise.all([
    events.transferProjectionUpdate.listen((event) => {
      useTransferStore.getState().applyProjection(event.payload);
    }),

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
      const { error } = event.payload;
      useTransferStore.getState().failSession(event.payload);
      if (error.startsWith("对方取消")) {
        toast.info(t`对方已取消传输`);
      } else {
        toast.error(error || t`传输失败`);
      }
    }),

    events.transferPaused.listen(() => {
      void useTransferStore.getState().loadProjections();
      toast.info(t`对方已暂停传输`);
    }),

    events.transferResumed.listen(() => {
      void useTransferStore.getState().loadProjections();
    }),

    events.transferAccepted.listen(() => {
      void useTransferStore.getState().loadProjections();
    }),

    events.transferRejected.listen((event) => {
      const { sessionId, reason } = event.payload;
      useTransferStore.getState().removeSession(sessionId);
      void useTransferStore.getState().loadProjections();
      if (reason?.type === "not_paired") {
        toast.error(t`设备已取消配对`);
      } else {
        toast.error(t`对方拒绝了请求`);
      }
    }),

    events.transferDbError.listen((event) => {
      toast.error(event.payload.message);
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

export const useTransferStore = create<TransferState>()((set, get) => ({
  projections: {},
  progressBySession: {},
  pendingOffers: [],

  applyProjection(projection) {
    set((state) => ({
      projections: {
        ...state.projections,
        [projection.sessionId]: projection,
      },
    }));
  },

  updateProgress(event) {
    set((state) => {
      const progressBySession = {
        ...state.progressBySession,
        [event.sessionId]: event,
      };
      const projection = state.projections[event.sessionId];
      const projections = projection
        ? {
            ...state.projections,
            [event.sessionId]: applyProgressToProjection(projection, event),
          }
        : state.projections;

      return { projections, progressBySession };
    });
  },

  completeSession() {
    void get().loadProjections();
  },

  failSession() {
    void get().loadProjections();
  },

  cancelSession() {
    void get().loadProjections();
  },

  removeSession(sessionId) {
    set((state) => {
      const { [sessionId]: _projection, ...projections } = state.projections;
      const { [sessionId]: _progress, ...progressBySession } =
        state.progressBySession;
      return { projections, progressBySession };
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
    return Object.values(get().projections).filter(isProjectionActive).length;
  },

  async loadProjections() {
    try {
      const items = await commands.getTransferProjections();
      set({
        projections: Object.fromEntries(
          items.map((item) => [item.sessionId, item]),
        ),
      });
    } catch (e) {
      console.error("加载传输投影失败:", e);
    }
  },
}));
