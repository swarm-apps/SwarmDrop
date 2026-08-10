import { create } from "zustand";
import {
  PROGRESS_STALE_MS,
  PUBLISH_VISIBLE_AFTER_MS,
  createSessionTimers,
  usableRates,
  type UsableRates,
} from "@swarmdrop/shared-view";
import {
  commands,
  events,
  type FilePublishEvent,
  type PrepareProgressEvent,
  type TransferOfferEvent,
  type TransferProgressEvent,
  type TransferProjection,
} from "@/lib/bindings";
import { setupTransferNotifications } from "@/lib/transfer-notifications";

/**
 * 正在从暂存位置发布到用户可见位置的那个文件。
 *
 * 「字节收完」不等于「文件已落地」：接收是暂存 → 发布两段，最后一帧进度打到 100% 之后
 * 发布才开始。桌面的发布是同目录 `rename`（O(1)），这条状态通常一闪而过——它存在是为了
 * 三端对同一件事有同一套表达，不是因为桌面会卡在这里。
 *
 * **正因为它一闪而过，它进这张表要晚 `PUBLISH_VISIBLE_AFTER_MS`**：见
 * [`schedulePublishReveal`]。表里有条目 ⇒ 这条发布已经久到值得解释了。
 */
export interface PublishingFile {
  fileId: number;
  name: string;
  relativePath: string;
  totalBytes: number;
}

/**
 * 一帧进度事件 + **它到达前端的时刻**。
 *
 * 到达时刻不是冗余记账：`TransferProgressEvent` 自己不带时间戳，而「这一帧还新不新鲜」
 * 是渲染速度与剩余时间的前置判据（`usableRates`）。后端的 `ProgressTracker::speed()`
 * 确实会在样本老于滑窗时归零，于是停滞之后的**下一帧**会诚实地带上 `speed: null` /
 * `eta: null`——问题是可能根本没有下一帧：进度事件只从收块路径发出，传输域里没有任何自走的
 * tick，对端一安静，最后那帧就永远躺在这张表里。不判时效的话，界面会把一个早已不成立的
 * 「12.4 MB/s · 剩余 45s」一直显示到会话超时：**它们不是在传输出问题时消失，而是在传输
 * 出问题时撒谎**，后者更糟。
 */
export interface SessionProgressFrame {
  event: TransferProgressEvent;
  /** `Date.now()`——本机收到这一帧的时刻，不是后端发出的时刻。 */
  receivedAt: number;
}

interface TransferState {
  projections: Record<string, TransferProjection>;
  progressBySession: Record<string, SessionProgressFrame>;
  /**
   * 每个会话当前正在发布的那个文件（没有则不存在该键）。
   *
   * **一个会话至多一条**：发布是「收齐即发布」，由接收 actor 顺序执行，同一会话不会有
   * 两个文件同时在发布。所以这里是 `sessionId → 文件` 而不是一张列表——列表的每一项都
   * 只会存活到下一条 `finished`，多出来的只有清理逻辑。
   *
   * 反过来它**不能**按 sessionId 之外的键存：发布是文件级事件，一个 100 文件的会话会发
   * 100 次、散布在整条传输里，不是末尾一次。
   */
  publishingBySession: Record<string, PublishingFile>;
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
  /**
   * 进度帧过了保鲜期，推一次会话级状态更新让订阅者重算。
   *
   * **定时器回调专用（[`armStaleTimer`]），组件不要调。** 它不改任何数值——这一帧的字节数
   * 和百分比仍是最后已知的真相，过期的只有速度与剩余时间，而那件事由渲染点的
   * [`useSessionRates`] 判。判据要被重新执行就得有人推一把：停滞时没有新事件 ⇒ 没有重渲染
   * ⇒ 判据永远不会再跑，界面会停在最后一帧画好的样子上。
   */
  ageProgress: (sessionId: string) => void;
  /** 文件发布事件：`started` 排延迟揭示、`finished` 收掉。 */
  applyFilePublish: (event: FilePublishEvent) => void;
  /**
   * 延迟揭示到点，把「正在保存」真正画出来。
   *
   * **定时器回调专用（[`schedulePublishReveal`]），组件不要调。**
   */
  revealPublishing: (sessionId: string, file: PublishingFile) => void;
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

      // 文件发布（暂存 → 用户可见位置）。与进度事件分开是因为它是**文件级**的：
      // 收齐即发布，一个会话里会发生多次、散布在整条传输过程中。
      events.filePublish.listen((event) => {
        useTransferStore.getState().applyFilePublish(event.payload);
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
  clearAllSessionTimers();
}

/* ─── 会话级定时器 ─── */

/**
 * 注入给台账原语的调度器。
 *
 * ⚠️ **必须包一层，不能直接传 `setTimeout` / `clearTimeout`。** 直接传等于在**模块加载时**
 * 把当时的全局函数捕成引用，而 `vi.useFakeTimers()` 是之后才替换 `globalThis.setTimeout`
 * 的——排出去的定时器会挂在真实时钟上，`advanceTimersByTime` 推不动，症状是「到点该发生
 * 的事」在测试里成片不发生，而生产代码看着完全正常。包一层让全局在**调用时**才解析。
 */
const setTimer = (fire: () => void, delayMs: number) =>
  setTimeout(fire, delayMs);
const clearTimer = (handle: ReturnType<typeof setTimeout>) =>
  clearTimeout(handle);

/**
 * 这两个台账里的定时器**不是状态**，是「没有新事件时也必须发生一次的事」——所以它们住在
 * 模块作用域而不是 store：进 state 只会让每个订阅者为一条自己看不见的记录白重算一轮。
 *
 * 台账原语来自 `@swarmdrop/shared-view`（三端同一份）。**调度器是参数**：那个包零平台依赖，
 * `setTimeout` 在它的 tsconfig 里根本不存在。它替这里担保两条语义——同会话重排先撤掉在途
 * 那条（「从现在起再等」而不是「再加一条」）、回调触发前先把自己从台账里摘掉（回调里往往
 * 会重新排一条，顺序反了会把新排的当旧句柄删掉）。
 *
 * 两条都按 sessionId 至多一条，收口在 [`clearSessionTimers`]。**三个路径都必须清**：
 * 会话离开 active、全量刷新后 `retain`、监听器整体拆除——否则定时器会追着一条已经死掉的
 * 会话继续推状态更新。收进共享包之前三端各写了一份且已经漂：批量清理只有移动端有、
 * 桌面把它内联展开、Web 干脆没有。
 */
const staleTimers = createSessionTimers(setTimer, clearTimer);
const publishRevealTimers = createSessionTimers(setTimer, clearTimer);

/**
 * 重排「进度帧过保鲜期」的推送：每来一帧就往后推 `PROGRESS_STALE_MS`。
 *
 * 阈值来自 `@swarmdrop/shared-view`，与渲染点判时效用的是同一个常数——本端再写一个
 * 6000 就等于三端各有一套保鲜期。
 */
function armStaleTimer(sessionId: string) {
  staleTimers.schedule(sessionId, PROGRESS_STALE_MS, () => {
    useTransferStore.getState().ageProgress(sessionId);
  });
}

/**
 * 排一条「延迟揭示」：满 `PUBLISH_VISIBLE_AFTER_MS` 还没结束的发布才值得画出来。
 *
 * 桌面的发布是同卷 `rename`，`started` 与 `finished` 背靠背到达却是**两条独立事件、
 * 两次渲染**，急着画就会让进度条每收齐一个文件闪一下灰——收一个几百个小文件的目录时
 * 就是持续频闪。真正需要解释的发布（Android SAF 全量拷贝）都远长于这个阈值。
 */
function schedulePublishReveal(event: FilePublishEvent) {
  const file: PublishingFile = {
    fileId: event.fileId,
    name: event.name,
    relativePath: event.relativePath,
    totalBytes: event.totalBytes,
  };
  publishRevealTimers.schedule(
    event.sessionId,
    PUBLISH_VISIBLE_AFTER_MS,
    () => {
      useTransferStore.getState().revealPublishing(event.sessionId, file);
    },
  );
}

/** 某个会话不再需要被推醒时的统一收口。 */
function clearSessionTimers(sessionId: string) {
  staleTimers.cancel(sessionId);
  publishRevealTimers.cancel(sessionId);
}

function clearAllSessionTimers() {
  staleTimers.clear();
  publishRevealTimers.clear();
}

/**
 * 按 sessionId 订阅单个会话的进度快照（无则 null）。
 * 进度事件高频回流，统一走这个入口把重渲染隔离到单个组件。
 *
 * 只给事件本身：绝大多数消费者要的是字节数与逐文件状态，**它们不随保鲜期推送而变**
 * （[`ageProgress`] 换的是外层包装，里面这个引用原样不动），所以那次推送不会把整个
 * 详情面板和会话行都重渲染一遍。
 *
 * **速度不要从这里读**——它有保质期，走 [`useSessionRates`]。
 */
export function useSessionProgress(
  sessionId: string,
): TransferProgressEvent | null {
  return useTransferStore((s) => s.progressBySession[sessionId]?.event ?? null);
}

/**
 * 按 sessionId 订阅这条会话**还能拿出来给人看**的速度与剩余时间（无则两个都是 null）。
 *
 * **速度与剩余时间必须一起过期，所以它们只能从同一个入口拿。** 两者同源于后端同一个滑窗，
 * 「这一帧太旧」对它们是同一件事；分开判的后果是同一行里一个诚实一个撒谎——剩余时间已经
 * 退成「计算中」，旁边还写着 `12.4 MB/s`，比两个都冻住更像 bug。做成一个返回两个数的 hook
 * 而不是让调用点各判各的，是因为「记得判两次」靠不住：桌面的速度那格就漏过一次。
 *
 * **时效在渲染那一刻算，不信任 store 里存了什么**：`Date.now()` 是唯一不会迟到的时钟，
 * store 那侧的定时器（[`armStaleTimer`]）只负责在陈旧那一刻把订阅者推醒一次、让这里重算。
 *
 * 已传字节 / 百分比**不在此列**：它们是累计量，作废会让进度条倒退，所以那些消费者走
 * [`useSessionProgress`]，也因此不跟着保鲜期推送重渲染。
 */
export function useSessionRates(sessionId: string): UsableRates {
  const frame = useTransferStore((s) => s.progressBySession[sessionId] ?? null);
  return usableRates(frame?.event, frame?.receivedAt, Date.now());
}

/** 当前活跃准备批次的进度快照（无则 null）。 */
export function useActivePrepareProgress(): PrepareProgressEvent | null {
  return useTransferStore((s) => s.activePrepare);
}

/**
 * 按 sessionId 订阅「正在保存的文件」（无则 null）。
 *
 * 返回的是 store 里那份对象的原引用，不派生新对象——它会被喂进 `memo` 化的会话行，
 * 每帧新建的对象会打穿 memo（也会踩到 `check:zustand-access` 的规则 B）。
 */
export function useSessionPublishing(sessionId: string): PublishingFile | null {
  return useTransferStore((s) => s.publishingBySession[sessionId] ?? null);
}

// 并发 loadProjections 的单调序号：迟到的旧快照不得覆盖新结果。
let loadSeq = 0;

/** 摘掉某个会话的「正在保存」条目；本来就没有就原样返回，不凭空造新引用。 */
function withoutPublishing(
  map: Record<string, PublishingFile>,
  sessionId: string,
): Record<string, PublishingFile> {
  if (!(sessionId in map)) return map;
  const { [sessionId]: _drop, ...rest } = map;
  return rest;
}

export const useTransferStore = create<TransferState>()((set) => ({
  projections: {},
  progressBySession: {},
  publishingBySession: {},
  activePrepare: null,
  clearedPreparedId: null,
  pendingOffers: [],
  dismissedOfferIds: [],

  applyProjection(projection) {
    // 定时器不是状态，收在 set 之外：会话一旦离开 active，既不该再被推醒（那些格子根本
    // 不渲染速度与剩余时间），也不该再冒出一条延迟揭示的「正在保存」。
    if (projection.phase !== "active") clearSessionTimers(projection.sessionId);

    set((state) => {
      const projections = {
        ...state.projections,
        [projection.sessionId]: projection,
      };

      // 「正在保存」只在会话仍是 active 时成立。发布失败的路径**刻意不发** `finished`
      // 事件（错误直接冒泡成可恢复的中断），所以这条清理是它唯一的收口；顺带也挡住了
      // 任何迟到事件留下的残影。
      //
      // 只挂在 projection 上、不另去订阅 TransferFailed / TransferPaused：projection 是
      // 生命周期的唯一权威源（后端每次状态转换都重发），下面待决 offer 的出队是同一个
      // 理由、同一个位置。
      const publishingBySession =
        projection.phase === "active"
          ? state.publishingBySession
          : withoutPublishing(state.publishingBySession, projection.sessionId);

      if (projection.phase !== "terminal") {
        return { projections, publishingBySession };
      }

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
        return { projections, progressBySession, publishingBySession };
      }
      return {
        projections,
        progressBySession,
        publishingBySession,
        pendingOffers,
        // 关闭标记跟着条目一起走，否则这张表会随被动结束的会话无界增长。
        dismissedOfferIds: state.dismissedOfferIds.filter(
          (id) => id !== projection.sessionId,
        ),
      };
    });
  },

  updateProgress(event) {
    // 保鲜期跟着最新一帧走，见 [`armStaleTimer`]。
    armStaleTimer(event.sessionId);
    // 进度只存 progressBySession 一处：活跃态 UI 读 progress，不再回写 projection
    // （回写既冗余又会被下一条 projection-update 覆盖，还每 tick churn 整个投影表）。
    set((state) => ({
      progressBySession: {
        ...state.progressBySession,
        [event.sessionId]: { event, receivedAt: Date.now() },
      },
    }));
  },

  ageProgress(sessionId) {
    set((state) => {
      const frame = state.progressBySession[sessionId];
      if (frame === undefined) return state;
      // 换掉外层包装、**一个数值都不改**：这一帧的字节数与百分比仍是最后已知的真相，
      // 过期的只有速度与剩余时间。里面 `event` 的引用原样传下去，于是只订阅事件的那些
      // 消费者（文件清单、字节数）拿到的快照不变，不跟着重渲染。
      return {
        progressBySession: {
          ...state.progressBySession,
          [sessionId]: { ...frame },
        },
      };
    });
  },

  /**
   * **按 sessionId 摘，不比 fileId。**
   *
   * 一条会话里的发布是**串行**的：`publish_file` 只有两个调用点，一个在
   * `handle_block_data`（跑在严格串行的收帧读循环里、`await` 到底），一个在
   * `publish_pending_empty_files`（`for` 循环逐个 `await`）；两条 `emit_publish_phase`
   * 都是直接 `await` 上报、不 spawn，宿主侧也只是同步转成 tauri 事件。所以同一会话的事件
   * 序列恒为 `started(f1) → finished(f1) → started(f2) → …`，两个文件的发布不会交叠，
   * 「上一个文件迟到的结束帧误伤当前条目」这个场景**构造不出来**。
   *
   * 于是按 fileId 比对是永真判据，只让台账多背一个字段、也让三端对同一条后端不变量做出
   * 两种相反的假设（Web 与移动都只按 sessionId 摘）。
   */
  applyFilePublish(event) {
    // 穷尽 switch、不留 default：将来加一档（比如「校验中」）会在这里编译期报缺项，
    // 而不是静默落进「当作发布结束」把提示收掉。守卫见函数末尾的 `satisfies never`。
    switch (event.phase) {
      case "started":
        // 不立刻画：常数时间的发布在阈值内就结束了，急着画就是每收齐一个文件闪一下灰。
        schedulePublishReveal(event);
        return;
      case "finished":
        // 还没揭示就结束——撤掉定时器即可，一帧都不闪（桌面走的就是这条路）。
        if (publishRevealTimers.cancel(event.sessionId)) return;
        set((state) => {
          const publishingBySession = withoutPublishing(
            state.publishingBySession,
            event.sessionId,
          );
          // 「内容没变」要 return state 而不是新对象：后者 `Object.is` 判不等，
          // 照样广播一轮。
          if (publishingBySession === state.publishingBySession) return state;
          return { publishingBySession };
        });
        return;
    }
    event.phase satisfies never;
  },

  revealPublishing(sessionId, file) {
    set((state) => {
      // **会话已经不活跃了就别显示。** 这是「凭空长出一条永不消失的正在保存」的唯一入口：
      // 若 `started` 因事件乱序落在会话终态之后，300ms 后这个定时器仍会把条目写回，而该
      // 会话再也不会有新的 projection 来清它 —— 条目就永久驻留了。宁可不显示。
      // （移动端一直有这道校验，桌面与 Web 此前没有，2026-08-10 对齐。）
      // 判据与下面清理发布态的那处逐字一致（`phase === "active"`），不要换成更宽的
      // `isProjectionActive` —— 两边判据不同就会出现「写得进、清不掉」。
      if (state.projections[sessionId]?.phase !== "active") return state;
      return {
        publishingBySession: { ...state.publishingBySession, [sessionId]: file },
      };
    });
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
        // 「正在保存」比进度多一条判据：会话还活着**且**仍是 active。全量刷新是重连 /
        // 进列表页的入口，此时增量事件可能整段丢过，残影只能靠这一刀清掉。
        const stillPublishing = new Set(
          items.filter((item) => item.phase === "active").map((i) => i.sessionId),
        );
        const publishingBySession = Object.fromEntries(
          Object.entries(state.publishingBySession).filter(([id]) =>
            stillPublishing.has(id),
          ),
        );
        return {
          projections: Object.fromEntries(
            items.map((item) => [item.sessionId, item]),
          ),
          progressBySession,
          publishingBySession,
        };
      });

      // 定时器按刷新后的状态重新对齐。全量刷新是重连 / 进列表页的入口，此时增量事件可能
      // 整段丢过——残留的定时器会追着一条已经不在表里的会话继续推状态更新，而上面那两刀
      // 只清得掉 state，清不掉 state 之外的东西。
      const live = useTransferStore.getState().projections;
      const isLive = (id: string) => live[id]?.phase === "active";
      staleTimers.retain(isLive);
      publishRevealTimers.retain(isLive);
    } catch (e) {
      console.error("加载传输投影失败:", e);
    }
  },
}));
