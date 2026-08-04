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

// 并发 loadProjections 的单调序号：迟到的旧快照不得覆盖新结果。
let loadSeq = 0;

export const useTransferStore = create<TransferState>()((set) => ({
  projections: {},
  progressBySession: {},
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
