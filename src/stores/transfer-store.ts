import { create } from "zustand";
import {
  commands,
  events,
  type PrepareProgressEvent,
  type TransferOfferEvent,
  type TransferProgressEvent,
  type TransferProjection,
} from "@/lib/bindings";
import { setupTransferNotifications } from "@/lib/transfer-notifications";

interface TransferState {
  projections: Record<string, TransferProjection>;
  progressBySession: Record<string, TransferProgressEvent>;
  /**
   * 当前活跃的发送准备批次（一遍流式读产出 checksum + 验签树），由**首条事件自我认领**。
   *
   * **不能挂进 `progressBySession`**：准备阶段还没有 `sessionId`，会话记录要等准备跑完、
   * 发出 Offer 时才创建。`preparedId` 也拿不到「提前」——它在 `prepareSend` 的返回值里，
   * 而事件先于返回值到达，所以认领只能由事件自己完成。
   *
   * **单个字段而不是一张 `preparedId → 快照` 表**：那张表唯一的读者就是「活跃的那一条」，
   * 非活跃条目从写进去到被删没有任何代码看过，却要为它付无上界增长、三端各写一套删键
   * 逻辑、以及每条非活跃事件白广播一轮订阅者的代价。
   */
  activePrepare: PrepareProgressEvent | null;
  /**
   * 最近一次被 [`clearPrepare`] 收掉的批次 id。
   *
   * 事件是**广播**的，投递路径与命令返回值不同，顺序无保证：收尾那条 100% 事件完全可能
   * 在 `clearPrepare()` 之后才到，于是活跃位被一个已经结束的批次重新占住，界面永久停在
   * 「正在准备 (n/n) 100%」。记住刚清掉的 id 就能把这类迟到事件挡回去。
   */
  clearedPreparedId: string | null;
  pendingOffers: TransferOfferEvent[];
  /**
   * 被用户关掉、但**没有被拒绝**的入站 offer。
   *
   * DESIGN.md 的 Incoming Request Contract：配对请求关闭 = 拒绝（对方在等这个答复），
   * 文件 offer 关闭 **≠** 拒绝（对方只是排着队，而误点一下作废整次传输的代价远大于
   * 多点一次）。所以关闭只把它挪到这里，条目仍留在 `pendingOffers` 里。
   *
   * 它必须住在 store 而不是弹窗的局部 state：契约同时要求「可关闭就必须有地方找回来」，
   * 而那个入口在收件箱页——两个组件得看同一份。
   */
  dismissedOfferIds: string[];

  applyProjection: (projection: TransferProjection) => void;
  updateProgress: (event: TransferProgressEvent) => void;
  updatePrepare: (event: PrepareProgressEvent) => void;
  /**
   * 收掉进度行。**无参**：调用点在失败路径上拿不到 `preparedId`（它在
   * `prepareSend` 的返回值里，而抛错时那个返回值不存在），带参数只会让兜底清理恒为空转。
   */
  clearPrepare: () => void;
  pushOffer: (offer: TransferOfferEvent) => void;
  /** 关闭（≠拒绝）：移出弹窗视野，仍可从收件箱找回。 */
  dismissOffer: (sessionId: string) => void;
  /** 从收件箱重新唤出一条已关闭的 offer。 */
  restoreOffer: (sessionId: string) => void;
  /** 真正处理完（接收 / 拒绝成功）后出队。**按 id 删，不能按队首**——用户关掉队首后
   *  弹窗展示的是队列里的下一条，此时出队队首会删错人。 */
  removeOffer: (sessionId: string) => void;
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
  // 状态同步订阅与 toast 通知订阅一起并发注册（都是独立 IPC listen，无先后依赖）。
  const [fns, unlistenNotifications] = await Promise.all([
    Promise.all([
      events.transferProjectionUpdate.listen((event) => {
        useTransferStore.getState().applyProjection(event.payload);
      }),

      events.transferOffer.listen((event) => {
        useTransferStore.getState().pushOffer(event.payload);
      }),

      events.transferProgress.listen((event) => {
        useTransferStore.getState().updateProgress(event.payload);
      }),

      // 发送准备进度。它是广播事件而非 per-call channel，所以 MCP 工具发起的准备、
      // 以及用户离开发送页之后的进度，在这里一样收得到。
      events.prepareProgress.listen((event) => {
        useTransferStore.getState().updatePrepare(event.payload);
      }),
    ]),
    setupTransferNotifications(),
  ]);

  unlistenFns = [...fns, unlistenNotifications];
}

export async function cleanupTransferListeners() {
  for (const unlisten of unlistenFns) {
    unlisten();
  }
  unlistenFns = [];
}

/**
 * 按 sessionId 订阅单个会话的进度快照（无则 null）。
 * 进度事件高频回流，统一走这个入口把重渲染隔离到单个组件。
 */
export function useSessionProgress(
  sessionId: string,
): TransferProgressEvent | null {
  return useTransferStore((s) => s.progressBySession[sessionId] ?? null);
}

/** 当前活跃准备批次的进度快照（无则 null）。 */
export function useActivePrepareProgress(): PrepareProgressEvent | null {
  return useTransferStore((s) => s.activePrepare);
}

// 并发 loadProjections 的单调序号：迟到的旧快照不得覆盖新结果。
let loadSeq = 0;

export const useTransferStore = create<TransferState>()((set) => ({
  projections: {},
  progressBySession: {},
  activePrepare: null,
  clearedPreparedId: null,
  pendingOffers: [],
  dismissedOfferIds: [],

  applyProjection(projection) {
    set((state) => {
      const projections = {
        ...state.projections,
        [projection.sessionId]: projection,
      };
      if (projection.phase !== "terminal") return { projections };

      // 终态会话清掉高频进度快照：避免无界堆积，也防止残留旧进度。
      const { [projection.sessionId]: _drop, ...progressBySession } =
        state.progressBySession;

      // **待决 offer 也随之出队。**
      //
      // `removeOffer` 只在用户 accept / reject 成功后调用，于是会话被**动**结束时那条
      // offer 会永久留在队列里：弹窗反复弹一条已经死掉的请求，点「接受」只会撞上内核的
      // 「会话不存在」。它被关闭后更持久——收件箱的「待处理请求」区是常驻列表，
      // 不像弹窗还能被关掉。
      //
      // 被动结束的三条真实来路：对端取消、对端下线、**本端决策窗口耗尽**
      // （`PENDING_OFFER_TIMEOUT_SECS`，内核清理任务发 `TimeoutSignal::OfferExpired`
      // 把会话推成 `terminal(expired)`）。最后一条是最常撞上的：170 秒没人管就到期，
      // 而在内核补上那条终态之前，这里写的清理**从来没有被触发过**——offer 卡死在列表里，
      // 接受和拒绝双双报错。
      //
      // projection 是生命周期的唯一权威源（后端每次状态转换都重发），所以清理挂在这里，
      // 而不是再去订阅 failed/cancelled 那几条冗余事件。
      const pendingOffers = state.pendingOffers.filter(
        (offer) => offer.sessionId !== projection.sessionId,
      );
      if (pendingOffers.length === state.pendingOffers.length) {
        return { projections, progressBySession };
      }
      return {
        projections,
        progressBySession,
        pendingOffers,
        // 关闭标记跟着条目一起走，否则这张表会随被动结束的会话无界增长。
        dismissedOfferIds: state.dismissedOfferIds.filter(
          (id) => id !== projection.sessionId,
        ),
      };
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

  updatePrepare(event) {
    set((state) => {
      // 刚被清掉的批次的迟到事件：丢弃，别让它重新占住活跃位。
      if (event.preparedId === state.clearedPreparedId) return state;
      const active = state.activePrepare;
      // 让位给新批次的三种情形：没有活跃批次、就是同一批、或者上一批已经跑到 100% 却
      // 没人清（MCP 工具发起的准备没有对应的前端调用点）。
      const canClaim =
        active === null ||
        active.preparedId === event.preparedId ||
        active.bytesHashed >= active.totalBytes;
      // 「内容没变」必须 return state 而不是 {}：后者是新对象，`Object.is` 判不等，
      // 照样广播一轮。
      return canClaim ? { activePrepare: event } : state;
    });
  },

  clearPrepare() {
    set((state) =>
      state.activePrepare === null
        ? state
        : { activePrepare: null, clearedPreparedId: state.activePrepare.preparedId },
    );
  },

  pushOffer(offer) {
    set((state) => ({
      pendingOffers: [...state.pendingOffers, offer],
    }));
  },

  dismissOffer(sessionId) {
    set((state) =>
      state.dismissedOfferIds.includes(sessionId)
        ? state
        : { dismissedOfferIds: [...state.dismissedOfferIds, sessionId] },
    );
  },

  restoreOffer(sessionId) {
    set((state) =>
      state.dismissedOfferIds.includes(sessionId)
        ? {
            dismissedOfferIds: state.dismissedOfferIds.filter(
              (id) => id !== sessionId,
            ),
          }
        : state,
    );
  },

  removeOffer(sessionId) {
    set((state) => {
      const pendingOffers = state.pendingOffers.filter(
        (offer) => offer.sessionId !== sessionId,
      );
      if (pendingOffers.length === state.pendingOffers.length) return state;
      // 一并摘掉它的关闭标记：同一个 sessionId 不会复用，留着只会让这张表无界增长。
      return {
        pendingOffers,
        dismissedOfferIds: state.dismissedOfferIds.filter(
          (id) => id !== sessionId,
        ),
      };
    });
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
