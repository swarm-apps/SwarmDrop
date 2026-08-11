import { t } from "@lingui/core/macro";
import {
  compareByTimelineDesc,
  createSessionTimers,
  isProgressFresh,
  PROGRESS_STALE_MS,
  PUBLISH_VISIBLE_AFTER_MS,
} from "@swarmdrop/shared-view";
import {
  type MobileFilePublish,
  MobileFilePublishPhase,
  type MobilePrepareProgress,
  type MobileTransferFile,
  type MobileTransferProgress,
  type MobileTransferProjection,
  type MobileTransferOffer as NativeTransferOffer,
} from "react-native-swarmdrop-core";
import { create } from "zustand";

import { getMobileCore } from "@/core/mobile-core";
import type { TransferOfferQueueItem } from "@/core/transfer-types";
import {
  isProjectionActive,
  isProjectionTerminal,
} from "@/core/transfer-types";
import { getErrorMessage } from "@/lib/errors";

/**
 * 一帧进度事件 + **它到达 JS 的时刻**。
 *
 * `receivedAt` 是渲染 ETA 的前置条件：后端 `speed()` 会在样本老于滑窗时归零，但**可能
 * 根本没有下一帧**（进度事件只从收块路径上发出，传输域里没有自走的 tick），于是对端一
 * 安静，最后那帧就永远躺在这里。渲染前一律过 `usableRates(frame, receivedAt, Date.now())`
 * ——保鲜期与判据都在 `@swarmdrop/shared-view`，本端不另存一份 6000。
 *
 * 「陈旧那一刻要有东西触发重算」由 [`ageProgressFrame`] 承担。
 */
export type ProgressFrame = MobileTransferProgress & {
  receivedAt: number;
};

/**
 * 正在发布（暂存 → 用户可见位置）的那个文件。
 *
 * 发布是**逐文件**的：收齐即发布，一个 100 文件的会话会发生 100 次、散布在整条传输里，
 * 不是末尾一次。所以按 sessionId 索引的是「这个会话此刻正在保存哪一个」，不是一份清单。
 */
export interface PublishingFile {
  fileId: number;
  name: string;
  relativePath: string;
  totalBytes: number;
  /**
   * 已搬运字节数。
   *
   * **只有 Android 的 SAF 目标会上报**——那条路径是全量字节拷贝（6 GB 的文件要写 12 GB）。
   * 其余平台（桌面 / Web / iOS / Android 的 `file://` 目标）的发布是 O(1) 重命名，
   * 没有循环可上报，这里恒为 0，UI 据此退到不确定态。
   */
  publishedBytes: number;
}

interface TransferState {
  /** 入站 offer 队列（接收方等用户响应；首条会被 transfer-offer-host 弹窗显示） */
  offerQueue: TransferOfferQueueItem[];
  currentOffer: TransferOfferQueueItem | null;

  /** TransferProjection 是 Activity/Recovery 的唯一状态源 */
  projections: Record<string, MobileTransferProjection>;
  progressBySession: Record<string, ProgressFrame>;

  /**
   * 每个会话当前正在发布的文件（没有则该 key 缺席）。
   *
   * **它不是进度事件的一部分**：最后一帧进度把条打到 100% 之后发布才开始，此时
   * `speed`/`eta` 已经没有新样本，把发布塞进 `progressBySession` 会让刚补好的 ETA 中毒。
   * 详见 openspec `transfer-eta-and-publish-feedback` 的 D2。
   *
   * **条目比 `started` 事件晚 `PUBLISH_VISIBLE_AFTER_MS` 才出现**（见
   * [`applyFilePublish`] / [`revealPublishing`]）：这一层是延迟揭示的唯一实现点，
   * 三个表面都读它，不必各自再判一次。
   */
  publishingBySession: Record<string, PublishingFile>;

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
  /**
   * **内部动作**，由 [`updateProgress`] 排的定时器在保鲜期到点时调用。
   *
   * 只做「渲染前过 `usableRates`」是不够的：停滞时没有新事件 ⇒ 没有重渲染 ⇒ 界面永远停在
   * 最后一帧画好的样子上，那条早已不成立的「剩余 45s」会一直显示到会话超时。这里在陈旧
   * 那一刻换一次对象身份（内容一字不改），memo 化的卡片据此重新求值，此时 `usableRates`
   * 已经把 eta 与速度一起判成 null、两格落到占位。
   */
  ageProgressFrame(sessionId: string): void;
  updatePrepare(snapshot: MobilePrepareProgress): void;
  /**
   * 消费 core 的文件发布事件（`started` 排一次延迟揭示、`finished` 收条目）。
   *
   * `finished` 会**撤掉还没揭示的那条**：桌面 / Web / iOS 的发布是常数时间（同卷
   * rename、OPFS close），`started` 与 `finished` 就是在 `PUBLISH_VISIBLE_AFTER_MS`
   * 之内背靠背走完的，撤掉等于「这一次根本没显示过」——急着画的代价是收一个几百个小
   * 文件的目录时进度条持续频闪。
   */
  applyFilePublish(event: MobileFilePublish): void;
  /** **内部动作**，由 [`applyFilePublish`] 排的延迟揭示定时器调用。 */
  revealPublishing(sessionId: string, file: PublishingFile): void;
  /**
   * Android SAF 拷贝循环的字节上报。
   *
   * **按 `relativePath` 认领已有条目**——上报方（`ForeignFileAccess`）拿到的元数据里
   * 没有 sessionId / fileId，归属只能由先到的 `started` 事件建立。匹配不上就丢弃，
   * **不凭空造条目**：那说明 `started` 还没到，后续帧会补上。
   *
   * 已知边界：两个会话同时在保存同名文件时会认领到先建的那条，代价是其中一张卡片的
   * 百分比错几秒。不为此把 sessionId 传下去——那要动三端共用的 `FileAccess` 端口签名。
   *
   * **返回「这个 `relativePath` 此刻有没有可展示的发布条目」。**「哪些发布值得播报」
   * 这条判据的唯一所有者就是本表：Rust 对小文件根本不发 `started`，而条目又只在延迟
   * 揭示之后才出现。上报方据此决定要不要更新 Android 前台通知，不再自己复刻一遍
   * 「揭示阈值」与「空文件」两条判据——那正是同一判据存两份、注释自认「必须一起改」
   * 的来源。
   */
  reportPublishBytes(relativePath: string, written: number): boolean;
  /** 收掉某会话的发布态（失败 / 暂停 / 会话消失）。 */
  clearPublishing(sessionId: string): void;
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

/**
 * 「这帧该判陈旧了」的定时器 —— 每收一帧进度重排一次。
 *
 * 会话消失（终态 / 记录被删 / 列表重载 / reset）时一并撤掉：留着虽然不会画错东西
 * （回调找不到帧就原样返回），但会让一个已经结束的会话在几秒后还惊动一次订阅者。
 */
const progressStaleTimers = createSessionTimers(
  (fire, delayMs) => setTimeout(fire, delayMs),
  (handle) => clearTimeout(handle),
);

/** 发布态的延迟揭示定时器 —— 见 [`applyFilePublish`]。 */
const publishRevealTimers = createSessionTimers(
  (fire, delayMs) => setTimeout(fire, delayMs),
  (handle) => clearTimeout(handle),
);

export const useTransferStore = create<TransferState & TransferActions>()(
  (set, get) => ({
    offerQueue: [],
    currentOffer: null,
    projections: {},
    progressBySession: {},
    publishingBySession: {},
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
      // 定时器先撤再改状态：它们的回调会往回写 store，留一条在途的就等于给一个已经
      // 结束的会话预约了一次「几秒后再出现一下」。
      // 判据是**非活跃**而不是终态，两张表一致（桌面与 Web 亦然）。suspended 的会话同样
      // 不渲染 ETA / 速度，留着保鲜期定时器只会在几秒后白广播一轮、把整张活动列表的 memo
      // 打穿一次。恢复传输时新的进度帧会重新排它，撤掉不会丢任何东西。
      if (!isProjectionActive(projection)) {
        progressStaleTimers.cancel(projection.sessionId);
        publishRevealTimers.cancel(projection.sessionId);
      }
      set((state) => {
        const patch: Partial<TransferState> = {
          projections: {
            ...state.projections,
            [projection.sessionId]: projection,
          },
        };
        // 终态会话清掉高频进度快照：避免无界堆积，也防止残留旧进度/速度。
        if (isProjectionTerminal(projection)) {
          const { [projection.sessionId]: _drop, ...progressBySession } =
            state.progressBySession;
          patch.progressBySession = progressBySession;
        }
        // 非活跃会话不可能还在发布。**这是发布失败的唯一出口**：`publish_file` 出错时
        // core 只让错误冒泡成可恢复的 Interrupted，不补发 `finished` 事件。
        if (
          !isProjectionActive(projection) &&
          projection.sessionId in state.publishingBySession
        ) {
          const { [projection.sessionId]: _stale, ...publishingBySession } =
            state.publishingBySession;
          patch.publishingBySession = publishingBySession;
        }
        return patch;
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
        // 发布只发生在活跃会话上，所以这里的存活判据比进度那条更窄。
        const live = new Set(items.map((item) => item.sessionId));
        const stillActive = new Set(
          items.filter(isProjectionActive).map((item) => item.sessionId),
        );
        // 两张表清到哪，定时器就撤到哪（两者的存活判据必须同源）。
        progressStaleTimers.retain((id) => live.has(id));
        publishRevealTimers.retain((id) => stillActive.has(id));
        set((state) => {
          return {
            projections: Object.fromEntries(
              items.map((item) => [item.sessionId, item]),
            ),
            progressBySession: Object.fromEntries(
              Object.entries(state.progressBySession).filter(([id]) =>
                live.has(id),
              ),
            ),
            publishingBySession: Object.fromEntries(
              Object.entries(state.publishingBySession).filter(([id]) =>
                stillActive.has(id),
              ),
            ),
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
      //
      // **到达时刻在这里记**：它是这一帧唯一拿得到的时间锚点，渲染时再取就成了「渲染
      // 时刻」，永远新鲜、等于没判。
      const { sessionId } = snapshot;
      const frame: ProgressFrame = { ...snapshot, receivedAt: Date.now() };
      set((state) => ({
        progressBySession: { ...state.progressBySession, [sessionId]: frame },
      }));
      // 闭包只捕获 sessionId：捕获 `snapshot` 会让整帧（含 files 数组）被在途定时器
      // 一并钉住 —— 高频事件下白留一份，且这一帧本就已经存进 store 了。
      progressStaleTimers.schedule(sessionId, PROGRESS_STALE_MS, () =>
        get().ageProgressFrame(sessionId),
      );
    },

    ageProgressFrame(sessionId) {
      const frame = get().progressBySession[sessionId];
      if (frame === undefined) return;
      if (isProgressFresh(frame.receivedAt, Date.now())) {
        // 定时器早到（宿主时钟抖动）。直接返回就再也没有下一次了 —— 补排一次，
        // 判据仍然是 shared-view 的那一条，这里不自己算「还差多少毫秒」。
        progressStaleTimers.schedule(sessionId, PROGRESS_STALE_MS, () =>
          get().ageProgressFrame(sessionId),
        );
        return;
      }
      // 内容一字不改，只换对象身份：这是 memo 化的卡片在「没有新事件」时唯一会重新
      // 求值的信号，重算后 usableEta 返回 null、ETA 那格落到占位。
      set((state) => {
        const current = state.progressBySession[sessionId];
        if (current === undefined) return state;
        return {
          progressBySession: {
            ...state.progressBySession,
            [sessionId]: { ...current },
          },
        };
      });
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

    applyFilePublish(event) {
      const { sessionId } = event;
      switch (event.phase) {
        case MobileFilePublishPhase.Finished:
          // 撤在途的揭示定时器 + 收已揭示的条目，两件事都要做：这一次发布走完时它
          // 可能还没显示（常数时间的那三端），也可能已经显示了（Android SAF 拷贝）。
          get().clearPublishing(sessionId);
          return;
        case MobileFilePublishPhase.Started: {
          // 闭包只捕获 sessionId 与这份快照，不留整个 event。
          const file: PublishingFile = {
            fileId: event.fileId,
            name: event.name,
            relativePath: event.relativePath,
            totalBytes: Number(event.totalBytes),
            publishedBytes: 0,
          };
          publishRevealTimers.schedule(
            sessionId,
            PUBLISH_VISIBLE_AFTER_MS,
            () => get().revealPublishing(sessionId, file),
          );
          return;
        }
        default: {
          // `phase` 是 fieldless uniffi enum，加第三档时这里编译期报缺项。上一版按
          // 字符串比，漏一档只会静默什么都不显示。
          const exhaustive: never = event.phase;
          return exhaustive;
        }
      }
    },

    revealPublishing(sessionId, file) {
      set((state) => {
        const projection = state.projections[sessionId];
        // 会话已经不活跃了：这是「凭空长出一条永不消失的正在保存」的唯一入口，宁可
        // 不显示。投影还没到（缺席）不算不活跃 —— 接收会话的首条投影可能比发布事件晚。
        if (projection && !isProjectionActive(projection)) return state;
        return {
          publishingBySession: {
            ...state.publishingBySession,
            [sessionId]: file,
          },
        };
      });
    },

    reportPublishBytes(relativePath, written) {
      const hit = Object.entries(get().publishingBySession).find(
        ([, entry]) => entry.relativePath === relativePath,
      );
      // 认领不到就丢弃：`started` 还没到、还没到揭示阈值、或该会话刚被收掉。
      // 后续帧会补上，**不凭空造条目**。
      if (!hit) return false;
      const [sessionId, entry] = hit;
      // 内容没变就一个字都不写：多余的 setState 会广播一轮。但条目在，仍然算「可展示」。
      if (entry.publishedBytes !== written) {
        set((state) => ({
          publishingBySession: {
            ...state.publishingBySession,
            [sessionId]: { ...entry, publishedBytes: written },
          },
        }));
      }
      return true;
    },

    clearPublishing(sessionId) {
      // 先撤定时器：还没揭示的那条如果不撤，会在 300ms 后把刚清掉的横幅又画回来。
      publishRevealTimers.cancel(sessionId);
      set((state) => {
        if (!(sessionId in state.publishingBySession)) return state;
        const { [sessionId]: _drop, ...publishingBySession } =
          state.publishingBySession;
        return { publishingBySession };
      });
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
        progressStaleTimers.cancel(sessionId);
        publishRevealTimers.cancel(sessionId);
        set((state) => {
          const { [sessionId]: _projection, ...projections } =
            state.projections;
          const { [sessionId]: _progress, ...progressBySession } =
            state.progressBySession;
          const { [sessionId]: _publishing, ...publishingBySession } =
            state.publishingBySession;
          return { projections, progressBySession, publishingBySession };
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
      progressStaleTimers.clear();
      publishRevealTimers.clear();
      set({
        offerQueue: [],
        currentOffer: null,
        projections: {},
        progressBySession: {},
        publishingBySession: {},
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
    .sort(compareByTimelineDesc)
    .map((projection) => projection.sessionId);
}
