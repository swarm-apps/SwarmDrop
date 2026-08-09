import { t } from "@lingui/core/macro";
import type {
  MobilePrepareProgress,
  MobileTransferFile,
  MobileTransferProgress,
  MobileTransferProjection,
  MobileTransferOffer as NativeTransferOffer,
} from "react-native-swarmdrop-core";
import { create } from "zustand";

import { getMobileCore } from "@/core/mobile-core";
import type { TransferOfferQueueItem } from "@/core/transfer-types";
import {
  compareProjectionsByUpdatedAtDesc,
  isProjectionActive,
  isProjectionTerminal,
} from "@/core/transfer-types";
import { getErrorMessage } from "@/lib/errors";

interface TransferState {
  /** 入站 offer 队列（接收方等用户响应；首条会被 transfer-offer-host 弹窗显示） */
  offerQueue: TransferOfferQueueItem[];
  currentOffer: TransferOfferQueueItem | null;

  /** TransferProjection 是 Activity/Recovery 的唯一状态源 */
  projections: Record<string, MobileTransferProjection>;
  progressBySession: Record<string, MobileTransferProgress>;

  /**
   * 当前活跃的发送准备批次（一遍流式读产出 checksum + 验签树），由**首条事件自我认领**。
   *
   * **不能挂进 `progressBySession`**：准备阶段还没有 `sessionId`，会话记录要等准备跑完、
   * 发出 Offer 时才创建。`preparedId` 也拿不到「提前」——它在 `prepareSend` 的返回值里，
   * 而事件先于返回值到达。
   *
   * **单个字段而不是一张按 id 索引的表**：那张表唯一的读者就是「活跃的那一条」，
   * 非活跃条目没有任何代码看过（三端同此形状）。
   */
  activePrepare: MobilePrepareProgress | null;
  /**
   * 最近一次被 `clearPrepare` 收掉的批次 id。
   *
   * 事件经 uniffi 的 foreign callback 投递，与 `prepareSend` 的 promise resolve 是两条路，
   * 顺序无保证：收尾那条 100% 事件可能在清理之后才到，于是活跃位被一个已结束的批次重新
   * 占住。这一页的进度条会顶掉发送按钮，那样就再也点不动了。
   */
  clearedPreparedId: string | null;

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
  updatePrepare(snapshot: MobilePrepareProgress): void;
  /**
   * 收掉进度行。**无参**：调用点在失败路径上拿不到 `preparedId`（它在 `prepareSend` 的
   * 返回值里，而抛错时那个返回值不存在）。
   */
  clearPrepare(): void;
  refreshAfterTransition(sessionId: string): Promise<void>;

  startSend(input: {
    files: MobileTransferFile[];
    peerId: string;
    peerName: string;
  }): Promise<string>;

  clearAllHistory(): Promise<void>;
  deleteHistoryItem(sessionId: string): Promise<void>;
  getSourcePaths(sessionId: string): Promise<string[]>;
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
    activePrepare: null,
    clearedPreparedId: null,
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
          origin: offer.origin,
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
          getErrorMessage(err),
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
          getErrorMessage(err),
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

    updatePrepare(snapshot) {
      set((state) => {
        // 刚被清掉的批次的迟到事件：丢弃，别让它重新占住活跃位。
        if (snapshot.preparedId === state.clearedPreparedId) return state;
        const active = state.activePrepare;
        // 让位给新批次的三种情形：没有活跃批次、就是同一批、或者上一批已经跑到 100% 却
        // 没人清。
        const canClaim =
          active === null ||
          active.preparedId === snapshot.preparedId ||
          active.bytesHashed >= active.totalBytes;
        // 「内容没变」要 return state 而不是 {}——后者是新对象，zustand 判不等照样广播。
        return canClaim ? { activePrepare: snapshot } : state;
      });
    },

    clearPrepare() {
      set((state) =>
        state.activePrepare === null
          ? state
          : {
              activePrepare: null,
              clearedPreparedId: state.activePrepare.preparedId,
            },
      );
    },

    async refreshAfterTransition(sessionId) {
      await get().loadProjection(sessionId);
    },

    async startSend(input) {
      // 开工先清：上一批可能是中途失败停在半路的，而认领规则只让「已跑到 100%」的让位。
      get().clearPrepare();
      try {
        const prepared = await getMobileCore().prepareSend(input.files);
        // 准备已完成：立刻收掉进度条。随后的 sendPrepared / loadProjection 不是准备，
        // 让它停在 100% 却还说着「正在准备」是在撒谎。
        get().clearPrepare();
        const result = await getMobileCore().sendPrepared(
          prepared.preparedId,
          input.peerId,
          input.peerName,
          // sendPrepared 的 fileIds 是子集筛选；当前 UI 没有子集 UI，必须传全量。
          prepared.files.map((f) => f.fileId),
        );
        await get().loadProjection(result.sessionId);
        return result.sessionId;
      } finally {
        // 兜底：prepare 自己抛错时上面那次 clear 根本没跑到。**必须无条件调**——
        // 早先这里写成 `if (preparedId)`，而 `preparedId` 恰恰只在 prepare 成功后才有值。
        get().clearPrepare();
      }
    },

    async clearAllHistory() {
      try {
        await getMobileCore().clearTransferActivity();
      } catch (err) {
        set({ lastError: getErrorMessage(err) });
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
        set({ lastError: getErrorMessage(err) });
      }
    },

    // 「重新发送」重建载荷用的源文件路径。取不到（接收会话 / 没记源路径的旧会话 /
    // 节点未启动）一律返回空数组，让调用方走「重新挑文件」的回退分支而不是弹错。
    async getSourcePaths(sessionId) {
      try {
        return await getMobileCore().getTransferSourcePaths(sessionId);
      } catch (err) {
        set({ lastError: getErrorMessage(err) });
        return [];
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
        activePrepare: null,
        clearedPreparedId: null,
        lastError: null,
      });
    },
  }),
);

/** 当前活跃准备批次的进度快照（无则 null）。 */
export function useActivePrepareProgress(): MobilePrepareProgress | null {
  return useTransferStore((s) => s.activePrepare);
}

export function selectActiveProjectionIds(state: TransferState): string[] {
  return Object.values(state.projections)
    .filter(isProjectionActive)
    .sort(compareProjectionsByUpdatedAtDesc)
    .map((projection) => projection.sessionId);
}
