import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  PROGRESS_STALE_MS,
  PUBLISH_VISIBLE_AFTER_MS,
  usableRates,
  type UsableRates,
} from "@swarmdrop/shared-view";

// store 模块顶层 import 了 bindings（Tauri IPC）与通知桥接，测试里都不该真跑。
vi.mock("@/lib/bindings", () => ({
  commands: { getTransferProjections: vi.fn(async () => []) },
  events: {
    transferProjectionUpdate: { listen: vi.fn() },
    transferOffer: { listen: vi.fn() },
    transferProgress: { listen: vi.fn() },
    filePublish: { listen: vi.fn() },
  },
}));
vi.mock("@/lib/transfer-notifications", () => ({
  setupTransferNotifications: vi.fn(async () => () => {}),
}));

import type {
  FilePublishEvent,
  TransferOfferEvent,
  TransferProgressEvent,
  TransferProjection,
} from "@/lib/bindings";
import { cleanupTransferListeners, useTransferStore } from "./transfer-store";

function publish(
  sessionId: string,
  fileId: number,
  phase: FilePublishEvent["phase"],
): FilePublishEvent {
  return {
    sessionId,
    fileId,
    name: `f${fileId}.bin`,
    relativePath: `f${fileId}.bin`,
    totalBytes: 100,
    phase,
  };
}

function offer(sessionId: string): TransferOfferEvent {
  return {
    sessionId,
    peerId: `peer-${sessionId}`,
    deviceName: `Device ${sessionId}`,
    files: [
      {
        fileId: 1,
        name: "a.bin",
        relativePath: "a.bin",
        size: 1,
        isDirectory: false,
      },
    ],
    totalSize: 1,
    origin: { type: "human" },
    policyAction: null,
    policyReason: null,
  } as TransferOfferEvent;
}

function projection(
  sessionId: string,
  phase: TransferProjection["phase"],
): TransferProjection {
  return { sessionId, phase } as TransferProjection;
}

function progressEvent(
  sessionId: string,
  eta: number | null,
): TransferProgressEvent {
  return {
    sessionId,
    direction: "receive",
    totalFiles: 1,
    completedFiles: 0,
    totalBytes: 100,
    transferredBytes: 50,
    speed: 10,
    eta,
    files: [],
  };
}

/**
 * 渲染点怎么读速率就怎么读：判据在渲染那一刻算，不信任 store 里存了什么。
 *
 * 这是 `useSessionRates` 的无 React 镜像（那是个 hook，跑不到组件外）——**两边要一起改**，
 * 它盯的正是「速度与剩余时间是不是一起过期」。
 */
function readRates(sessionId: string): UsableRates {
  const frame = useTransferStore.getState().progressBySession[sessionId];
  return usableRates(frame?.event, frame?.receivedAt, Date.now());
}

beforeEach(async () => {
  // 假定时器全程开着：本文件里有两处「到点才发生」的行为（进度保鲜期、发布延迟揭示）。
  vi.useFakeTimers();
  // 上一条用例留下的会话级定时器一并清掉——它们住在模块作用域，setState 清不到。
  await cleanupTransferListeners();
  useTransferStore.setState({
    projections: {},
    progressBySession: {},
    publishingBySession: {},
    pendingOffers: [],
    dismissedOfferIds: [],
  });
});

afterEach(() => {
  vi.useRealTimers();
});

// 每条用例都先建一条 active 的 projection：发布只发生在传输过程中，`revealPublishing`
// 因此拒绝给一条不活跃（或根本不在册）的会话写条目——那是「凭空长出一条永不消失的正在
// 保存」的唯一入口（`started` 因事件乱序落在终态之后，300ms 后定时器仍会把它写回，而该
// 会话再也不会有新的 projection 来清它）。没有这个前置条件，用例测的是一个不可能的状态。
describe("文件发布态（暂存 → 用户可见位置）", () => {
  it("started 满延迟才写入、finished 收掉", () => {
    const s = useTransferStore.getState();
    s.applyProjection(projection("a", "active"));
    s.applyFilePublish(publish("a", 1, "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);

    expect(useTransferStore.getState().publishingBySession.a).toMatchObject({
      fileId: 1,
      name: "f1.bin",
    });

    s.applyFilePublish(publish("a", 1, "finished"));
    expect(useTransferStore.getState().publishingBySession).toEqual({});
  });

  // 常数时间的发布（桌面同卷 rename）里 started 与 finished 背靠背到达却是两条独立
  // 事件、两次渲染，急着画就是每收齐一个文件闪一下灰；收一个几百个小文件的目录时
  // 就是持续频闪。
  it("PUBLISH_VISIBLE_AFTER_MS 内结束的发布一格都不画", () => {
    const s = useTransferStore.getState();
    s.applyFilePublish(publish("a", 1, "started"));
    expect(useTransferStore.getState().publishingBySession.a).toBeUndefined();

    s.applyFilePublish(publish("a", 1, "finished"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS * 4);

    expect(useTransferStore.getState().publishingBySession.a).toBeUndefined();
  });

  // 同一条会话的发布是**串行**的（`publish_file` 的两个调用点都在串行读循环里 await 到底，
  // 上报也不 spawn），所以事件序列恒为 started(f1) → finished(f1) → started(f2) → …，
  // 两个文件的发布不会交叠。三端因此都按 sessionId 摘条目、不比 fileId；这条用例钉住的是
  // 「下一个文件揭示时，前一个已被自己的 finished 收干净」。
  it("串行发布：后一个文件揭示时，前一个的条目已经被收掉", () => {
    const s = useTransferStore.getState();
    s.applyProjection(projection("a", "active"));
    s.applyFilePublish(publish("a", 1, "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);
    expect(useTransferStore.getState().publishingBySession.a?.fileId).toBe(1);

    s.applyFilePublish(publish("a", 1, "finished"));
    expect(useTransferStore.getState().publishingBySession.a).toBeUndefined();

    s.applyFilePublish(publish("a", 2, "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);
    expect(useTransferStore.getState().publishingBySession.a?.fileId).toBe(2);
  });

  // 发布失败**不发** finished（错误直接冒泡成可恢复的中断），会话转 suspended /
  // terminal 是这条状态唯一的收口；迟到事件留下的残影也靠它。
  it("会话离开 active 时清空，active 的 projection 不动它", () => {
    const s = useTransferStore.getState();
    s.applyProjection(projection("a", "active"));
    s.applyFilePublish(publish("a", 1, "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);

    s.applyProjection(projection("a", "active"));
    expect(useTransferStore.getState().publishingBySession.a).toBeDefined();

    s.applyProjection(projection("a", "suspended"));
    expect(useTransferStore.getState().publishingBySession.a).toBeUndefined();
  });

  // 会话离开 active 时，**还没揭示**的那条也不能再冒出来。
  it("会话离开 active 时，待揭示的发布提示不再出现", () => {
    const s = useTransferStore.getState();
    s.applyFilePublish(publish("a", 1, "started"));
    s.applyProjection(projection("a", "terminal"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS * 4);

    expect(useTransferStore.getState().publishingBySession.a).toBeUndefined();
  });

  it("终态会话一并清掉发布态", () => {
    const s = useTransferStore.getState();
    s.applyProjection(projection("a", "active"));
    s.applyProjection(projection("b", "active"));
    s.applyFilePublish(publish("a", 1, "started"));
    s.applyFilePublish(publish("b", 1, "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);

    s.applyProjection(projection("a", "terminal"));

    const next = useTransferStore.getState().publishingBySession;
    expect(next.a).toBeUndefined();
    expect(next.b).toBeDefined();
  });
});

// 停滞时后端不会再发帧（进度事件只由收块路径驱动，传输域里没有自走的 tick），
// 最后那条「12.4 MB/s · 剩余 45s」会一直躺在 store 里——它们不是在传输出问题时消失，
// 而是在传输出问题时撒谎。判据（PROGRESS_STALE_MS / usableRates）来自 shared-view，
// 这里验的是「陈旧那一刻有没有人把订阅者推醒」：没有推送就没有重渲染，界面会停在最后一帧上。
describe("进度帧的保鲜期", () => {
  it("到点推一次会话级更新，且那一刻两个数已经一起不可用", () => {
    const s = useTransferStore.getState();
    s.updateProgress(progressEvent("a", 45));
    expect(readRates("a")).toEqual({ eta: 45, speed: 10 });

    const seen: UsableRates[] = [];
    const unsubscribe = useTransferStore.subscribe(() =>
      seen.push(readRates("a")),
    );
    vi.advanceTimersByTime(PROGRESS_STALE_MS);
    unsubscribe();

    // **速度与剩余时间必须一起过期**：只作废一个，同一行里就会是「12.4 MB/s · 计算中」，
    // 一半诚实一半撒谎，比两个都冻住更像 bug。
    expect(seen).toEqual([{ eta: null, speed: null }]);
  });

  it("推送不改数值，只换外层包装——事件引用原样传下去", () => {
    const s = useTransferStore.getState();
    const event = progressEvent("a", 45);
    s.updateProgress(event);

    vi.advanceTimersByTime(PROGRESS_STALE_MS);

    // 字节数与逐文件状态仍是最后已知的真相，过期的只有 ETA；引用不变 ⇒
    // 只订阅事件的消费者不跟着重渲染。
    expect(useTransferStore.getState().progressBySession.a?.event).toBe(event);
  });

  it("新的一帧把保鲜期整条往后推", () => {
    const s = useTransferStore.getState();
    s.updateProgress(progressEvent("a", 45));
    vi.advanceTimersByTime(PROGRESS_STALE_MS - 1000);
    s.updateProgress(progressEvent("a", 40));

    vi.advanceTimersByTime(PROGRESS_STALE_MS - 1);
    expect(readRates("a").eta).toBe(40);

    vi.advanceTimersByTime(1);
    expect(readRates("a")).toEqual({ eta: null, speed: null });
  });

  // 判据挑 suspended 而不是 terminal 是有意的：终态会连进度帧一起删掉，那时就算定时器
  // 漏清也推不出东西来，测不出「清没清」。suspended 会把帧留在表里，只有定时器真的被
  // 清掉才不会有推送——那一格本来也不渲染 ETA，没人需要被推醒。
  it("会话离开 active 后不再推：定时器跟着会话一起清掉", () => {
    const s = useTransferStore.getState();
    s.updateProgress(progressEvent("a", 45));
    s.applyProjection(projection("a", "suspended"));

    let notified = 0;
    const unsubscribe = useTransferStore.subscribe(() => {
      notified += 1;
    });
    vi.advanceTimersByTime(PROGRESS_STALE_MS * 2);
    unsubscribe();

    expect(notified).toBe(0);
  });

  // 全量刷新（重连 / 进列表页）是「增量事件可能整段丢过」的入口，也是两个定时器台账唯一的
  // 批量清理点：清 state 的那两刀清不掉 state 之外的东西，残留的定时器会追着一条已经不在
  // 表里的会话继续推状态更新。mock 的 getTransferProjections 恒返回空表 ⇒ 会话 a 已消失。
  it("全量刷新后，已不在表里的会话不再被推醒（两个台账都要 retain）", async () => {
    const s = useTransferStore.getState();
    s.updateProgress(progressEvent("a", 45));
    s.applyFilePublish(publish("a", 1, "started"));

    await s.loadProjections();

    let notified = 0;
    const unsubscribe = useTransferStore.subscribe(() => {
      notified += 1;
    });
    vi.advanceTimersByTime(
      Math.max(PROGRESS_STALE_MS, PUBLISH_VISIBLE_AFTER_MS) * 2,
    );
    unsubscribe();

    expect(notified).toBe(0);
  });
});

describe("入站 offer 队列", () => {
  it("关闭一条不影响它仍在队列里——关闭 ≠ 拒绝", () => {
    const s = useTransferStore.getState();
    s.pushOffer(offer("a"));
    s.dismissOffer("a");

    expect(useTransferStore.getState().pendingOffers).toHaveLength(1);
    expect(useTransferStore.getState().dismissedOfferIds).toEqual(["a"]);
  });

  it("按 id 出队，而不是按队首", () => {
    const s = useTransferStore.getState();
    s.pushOffer(offer("a"));
    s.pushOffer(offer("b"));
    // 用户关掉了队首，此时弹窗展示的是 b——出队必须删 b，不能删队首 a。
    s.dismissOffer("a");
    s.removeOffer("b");

    expect(
      useTransferStore.getState().pendingOffers.map((o) => o.sessionId),
    ).toEqual(["a"]);
  });

  it("出队时一并摘掉关闭标记，这张表不随会话无界增长", () => {
    const s = useTransferStore.getState();
    s.pushOffer(offer("a"));
    s.dismissOffer("a");
    s.removeOffer("a");

    expect(useTransferStore.getState().dismissedOfferIds).toEqual([]);
  });

  // 回归：`removeOffer` 只在用户 accept/reject 成功后调用，于是**对端取消**时那条 offer
  // 会永久留在队列里——弹窗反复弹一条死会话，点「接受」撞内核的「会话不存在」；被关闭之后
  // 更持久，因为收件箱的「待处理请求」是常驻列表。清理挂在 projection 上（生命周期唯一权威源）。
  it("会话转终态时，待决 offer 自动出队（对端取消 / 超时 / 对端下线）", () => {
    const s = useTransferStore.getState();
    s.pushOffer(offer("a"));
    s.pushOffer(offer("b"));
    s.dismissOffer("a");

    s.applyProjection(projection("a", "terminal"));

    const next = useTransferStore.getState();
    expect(next.pendingOffers.map((o) => o.sessionId)).toEqual(["b"]);
    expect(next.dismissedOfferIds).toEqual([]);
  });

  it("非终态的 projection 不动队列", () => {
    const s = useTransferStore.getState();
    s.pushOffer(offer("a"));
    s.applyProjection(projection("a", "active"));

    expect(useTransferStore.getState().pendingOffers).toHaveLength(1);
  });
});
