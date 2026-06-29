import { t } from "@lingui/core/macro";
import type {
  MobileTransferFile,
  MobileTransferProgress,
  MobileTransferProjection,
  MobileTransferOffer as NativeTransferOffer,
} from "react-native-swarmdrop-core";
import { create } from "zustand";

import { getMobileCore } from "@/core/mobile-core";
import type { TransferOfferQueueItem } from "@/core/transfer-types";
import {
  isProjectionActive,
  isProjectionTerminal,
} from "@/core/transfer-types";
import { errorMessage } from "@/lib/utils";

interface TransferState {
  /** 入站 offer 队列（接收方等用户响应；首条会被 transfer-offer-host 弹窗显示） */
  offerQueue: TransferOfferQueueItem[];
  currentOffer: TransferOfferQueueItem | null;

  /** TransferProjection 是 Activity/Recovery 的唯一状态源 */
  projections: Record<string, MobileTransferProjection>;
  progressBySession: Record<string, MobileTransferProgress>;

  /** 最近一次错误，主要给 toast 用 */
  lastError: string | null;
}

interface TransferActions {
  pushOffer(offer: NativeTransferOffer): void;
  dismissOffer(id: string): void;

  applyProjection(projection: MobileTransferProjection): void;
  loadProjection(sessionId: string): Promise<void>;
  loadProjections(): Promise<void>;

  updateProgress(snapshot: MobileTransferProgress): void;
  refreshAfterTransition(sessionId: string): Promise<void>;

  startSend(input: {
    files: MobileTransferFile[];
    peerId: string;
    peerName: string;
  }): Promise<string>;

  clearAllHistory(): Promise<void>;
  deleteHistoryItem(sessionId: string): Promise<void>;
  resumeHistoryItem(sessionId: string): Promise<string>;

  setError(message: string | null): void;
  reset(): void;
}

// 并发 loadProjections 的单调序号：迟到的旧快照不得覆盖新结果。
let loadSeq = 0;

export const useTransferStore = create<TransferState & TransferActions>()(
  (set, get) => ({
    offerQueue: [],
    currentOffer: null,
    projections: {},
    progressBySession: {},
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
          policyAction: offer.policyAction,
          policyReason: offer.policyReason,
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

    applyProjection(projection) {
      set((state) => {
        const projections = {
          ...state.projections,
          [projection.sessionId]: projection,
        };
        // 终态会话清掉高频进度快照：避免无界堆积，也防止残留旧进度/速度。
        if (isProjectionTerminal(projection)) {
          const { [projection.sessionId]: _drop, ...progressBySession } =
            state.progressBySession;
          return { projections, progressBySession };
        }
        return { projections };
      });
    },

    async loadProjection(sessionId) {
      try {
        const projection =
          await getMobileCore().getTransferProjection(sessionId);
        if (!projection) return;
        get().applyProjection(projection);
      } catch (err) {
        console.warn(
          "[transfer-store] loadProjection failed:",
          errorMessage(err),
        );
      }
    },

    async loadProjections() {
      const seq = ++loadSeq;
      try {
        const items = await getMobileCore().getTransferProjections();
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
      } catch (err) {
        console.warn(
          "[transfer-store] loadProjections failed:",
          errorMessage(err),
        );
        set({ lastError: t`加载传输活动失败` });
      }
    },

    updateProgress(snapshot) {
      // 进度只存 progressBySession 一处：展示用 projectionTransferredBytes(projection,
      // progress) 已优先读 progress，回写 projection 既冗余又会被下条 projection-update
      // 覆盖，还每 tick churn 整张投影表。
      set((state) => ({
        progressBySession: {
          ...state.progressBySession,
          [snapshot.sessionId]: snapshot,
        },
      }));
    },

    async refreshAfterTransition(sessionId) {
      await get().loadProjection(sessionId);
    },

    async startSend(input) {
      const prepared = await getMobileCore().prepareSend(input.files);
      const result = await getMobileCore().sendPrepared(
        prepared.preparedId,
        input.peerId,
        input.peerName,
        // sendPrepared 的 fileIds 是子集筛选；当前 UI 没有子集 UI，必须传全量。
        prepared.files.map((f) => f.fileId),
      );
      await get().loadProjection(result.sessionId);
      return result.sessionId;
    },

    async clearAllHistory() {
      try {
        await getMobileCore().clearTransferActivity();
      } catch (err) {
        set({ lastError: errorMessage(err) });
      } finally {
        await get().loadProjections();
      }
    },

    async deleteHistoryItem(sessionId) {
      try {
        await getMobileCore().deleteTransferRecord(sessionId);
        set((state) => {
          const { [sessionId]: _projection, ...projections } =
            state.projections;
          const { [sessionId]: _progress, ...progressBySession } =
            state.progressBySession;
          return { projections, progressBySession };
        });
      } catch (err) {
        set({ lastError: errorMessage(err) });
      }
    },

    async resumeHistoryItem(sessionId) {
      const projection = await getMobileCore().resumeTransfer(sessionId);
      get().applyProjection(projection);
      return projection.sessionId;
    },

    setError(message) {
      set({ lastError: message });
    },

    reset() {
      set({
        offerQueue: [],
        currentOffer: null,
        projections: {},
        progressBySession: {},
        lastError: null,
      });
    },
  }),
);

export function selectActiveProjectionIds(state: TransferState): string[] {
  return Object.values(state.projections)
    .filter(isProjectionActive)
    .sort((a, b) => Number(b.updatedAt - a.updatedAt))
    .map((projection) => projection.sessionId);
}
