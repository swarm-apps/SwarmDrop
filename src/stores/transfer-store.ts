import { create } from "zustand";
import {
  commands,
  events,
  type TransferOfferEvent,
  type TransferProgressEvent,
  type TransferProjection,
} from "@/lib/bindings";
import { setupTransferNotifications } from "@/lib/transfer-notifications";

interface TransferState {
  projections: Record<string, TransferProjection>;
  progressBySession: Record<string, TransferProgressEvent>;
  pendingOffers: TransferOfferEvent[];

  applyProjection: (projection: TransferProjection) => void;
  updateProgress: (event: TransferProgressEvent) => void;
  pushOffer: (offer: TransferOfferEvent) => void;
  shiftOffer: () => TransferOfferEvent | undefined;
  loadProjections: () => Promise<void>;
}

let unlistenFns: Array<() => void> = [];

export async function setupTransferListeners() {
  await cleanupTransferListeners();

  await useTransferStore.getState().loadProjections();

  // 后端在每次状态迁移都发 transferProjectionUpdate（accept/pause/resume/complete/
  // fail/cancel/reject），它是唯一权威状态源，由 applyProjection 增量合并进 store。
  // loadProjections 仅用于初始化、进入列表页、以及删除路径（增量事件无法表达删除）。
  // 纯 toast 副作用（failed/paused/rejected/dbError）拆到 setupTransferNotifications，
  // 这里只保留 projection / progress / offer 的状态同步订阅。
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
  ]);

  const unlistenNotifications = await setupTransferNotifications();

  unlistenFns = [...fns, unlistenNotifications];
}

export async function cleanupTransferListeners() {
  for (const unlisten of unlistenFns) {
    unlisten();
  }
  unlistenFns = [];
}

// 并发 loadProjections 的单调序号：迟到的旧快照不得覆盖新结果。
let loadSeq = 0;

export const useTransferStore = create<TransferState>()((set, get) => ({
  projections: {},
  progressBySession: {},
  pendingOffers: [],

  applyProjection(projection) {
    set((state) => {
      const projections = {
        ...state.projections,
        [projection.sessionId]: projection,
      };
      // 终态会话清掉高频进度快照：避免无界堆积，也防止残留旧进度。
      if (projection.phase === "terminal") {
        const { [projection.sessionId]: _drop, ...progressBySession } =
          state.progressBySession;
        return { projections, progressBySession };
      }
      return { projections };
    });
  },

  updateProgress(event) {
    // 进度只存 progressBySession 一处：活跃态 UI 读 progress，不再回写 projection
    // （回写既冗余又会被下一条 projection-update 覆盖，还每 tick churn 整个投影表）。
    set((state) => ({
      progressBySession: {
        ...state.progressBySession,
        [event.sessionId]: event,
      },
    }));
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

  async loadProjections() {
    const seq = ++loadSeq;
    try {
      const items = await commands.getTransferProjections();
      // 丢弃过期快照：有更新的 load 已发起就不覆盖（消除并发 reload 乱序）。
      if (seq !== loadSeq) return;
      set((state) => {
        const live = new Set(items.map((item) => item.sessionId));
        const progressBySession = Object.fromEntries(
          Object.entries(state.progressBySession).filter(([id]) =>
            live.has(id),
          ),
        );
        return {
          projections: Object.fromEntries(
            items.map((item) => [item.sessionId, item]),
          ),
          progressBySession,
        };
      });
    } catch (e) {
      console.error("加载传输投影失败:", e);
    }
  },
}));
