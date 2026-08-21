import { PROGRESS_STALE_MS, PUBLISH_VISIBLE_AFTER_MS } from "@swarmdrop/shared-view";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { webNodeStore, webNodeActions } from "./store";
import type { TransferProjection, WebTransferEvent } from "./view-types";

/**
 * 走 `applyEvent` 这个**真实入口**而不是导出内部的 `reduceEvent`：为测试放宽公开面，
 * 下一个人就会直接调那个内部函数，归约与 setState 的边界随即消失。
 */
function offerEvent(sessionId: string): WebTransferEvent {
  return {
    type: "transferOfferReceived",
    offer: {
      sessionId,
      peerId: `peer-${sessionId}`,
      deviceName: `Device ${sessionId}`,
      files: [],
      totalSize: 0,
    },
  } as unknown as WebTransferEvent;
}

function projectionEvent(
  sessionId: string,
  phase: TransferProjection["phase"],
): WebTransferEvent {
  return {
    type: "transferProjection",
    projection: { sessionId, phase },
  } as unknown as WebTransferEvent;
}

function prepareEvent(preparedId: string, bytesHashed = 0): WebTransferEvent {
  return {
    type: "prepareProgress",
    event: {
      preparedId,
      currentFile: "a.bin",
      completedFiles: 0,
      totalFiles: 1,
      bytesHashed,
      totalBytes: 100,
    },
  } as unknown as WebTransferEvent;
}

function publishEvent(
  sessionId: string,
  phase: "started" | "finished",
  name = "a.zip",
): WebTransferEvent {
  return {
    type: "filePublish",
    event: {
      sessionId,
      fileId: 1,
      name,
      relativePath: name,
      totalBytes: 100,
      phase,
    },
  } as unknown as WebTransferEvent;
}

function progressEvent(sessionId: string, eta = 45): WebTransferEvent {
  return {
    type: "transferProgress",
    event: {
      sessionId,
      totalFiles: 1,
      completedFiles: 0,
      totalBytes: 100,
      transferredBytes: 30,
      speed: 10,
      eta,
      files: [],
    },
  } as unknown as WebTransferEvent;
}

function textAttentionEvent(
  kind: "confirmation_required" | "received",
): WebTransferEvent {
  return {
    type: "textDeliveryAttention",
    attention: {
      deliveryId: "delivery-1",
      peerId: "peer-1",
      peerName: "Alice",
      kind,
      createdAt: 1,
    },
  } as unknown as WebTransferEvent;
}

beforeEach(() => {
  // 每条用例都在假时钟上跑：本 store 的两条时效判据（进度保鲜期、发布延迟揭示）都由
  // `setTimeout` 驱动，真时钟下要么等 6 秒要么根本测不到。
  vi.useFakeTimers();
  webNodeStore.setState({
    offers: {},
    projections: {},
    eventLog: [],
    activePrepare: null,
    clearedPreparedId: null,
    progress: {},
    progressAt: {},
    publishingBySession: {},
  });
});

afterEach(() => {
  // 先把在途定时器收干净再还原时钟——否则上一条用例排的发布揭示会在下一条里醒来。
  webNodeActions.reset();
  vi.useRealTimers();
});

function inboxEvent(
  type: "inboxItemAdded" | "inboxItemArchived" | "inboxItemRemoved",
): WebTransferEvent {
  return {
    type,
    event: { itemId: "item-1", contentKind: "files" },
  } as unknown as WebTransferEvent;
}

describe("失效票据的两条来源互不相干", () => {
  /**
   * **注意力信号不再顺手失效收件箱。**
   *
   * 它此前那么做，是因为当时没有别的信号可用——而它只覆盖文本，文件到达一条都推不出来。
   * 现在收件箱的变化由一等的 `inboxItemAdded` 系列发（spec: `inbox-domain-events`），
   * 两者回答的是不同的问题：注意力问「有人发了东西过来，要不要提示」，失效票据问
   * 「收件箱那张表还准不准」。
   */
  it("注意力信号只顶注意力的计数器，不动收件箱", () => {
    const before = webNodeStore.getState();

    for (const kind of ["confirmation_required", "received"] as const) {
      webNodeActions.applyEvent(textAttentionEvent(kind));
    }

    expect(webNodeStore.getState().textDeliveryRevision).toBe(
      before.textDeliveryRevision + 2,
    );
    expect(webNodeStore.getState().inboxRevision).toBe(before.inboxRevision);
  });

  /**
   * **三条收件箱事件都要顶失效票据。**
   *
   * 载荷刻意很窄（不含标题——文本条目的标题就是正文前 160 字节，而事件会流经日志），
   * 所以它们能做的只有「让面板重拉一次真表」。漏掉其中任何一条，那类变化在界面上就
   * 完全不可见，且不报错。
   */
  it("每条收件箱事件都失效收件箱", () => {
    const types = [
      "inboxItemAdded",
      "inboxItemArchived",
      "inboxItemRemoved",
    ] as const;

    for (const type of types) {
      const before = webNodeStore.getState().inboxRevision;
      webNodeActions.applyEvent(inboxEvent(type));
      expect(webNodeStore.getState().inboxRevision).toBe(before + 1);
    }
  });

  /**
   * **传输完成不再推导收件箱变化。**
   *
   * 那条推导依赖「先建条目、再发完成事件」这条只以行内注释存在的顺序，而生产里终态
   * projection 比条目创建更早发出——重拉到的是一张还没有那条记录的表。
   */
  it("传输完成不再顶收件箱的失效票据", () => {
    const before = webNodeStore.getState().inboxRevision;
    webNodeActions.applyEvent({
      type: "transferCompleted",
      event: { sessionId: "s-1", direction: "receive" },
    } as unknown as WebTransferEvent);

    expect(webNodeStore.getState().inboxRevision).toBe(before);
  });
});

describe("待决 offer 的生命周期", () => {
  // 回归：`removeOffer` 只在用户 accept/reject 成功后调用，于是**对端取消**（或超时、
  // 或对端下线）时那条 offer 会永久留在 `offers` 里——全局对话框反复弹一条死会话，
  // 点「接受」撞内核的「会话不存在」。收件箱的「待处理请求」是常驻列表，让它更持久。
  it("会话转终态时自动清掉对应的 offer", () => {
    webNodeActions.applyEvent(offerEvent("a"));
    webNodeActions.applyEvent(offerEvent("b"));
    expect(Object.keys(webNodeStore.getState().offers).sort()).toEqual(["a", "b"]);

    webNodeActions.applyEvent(projectionEvent("a", "terminal"));

    expect(Object.keys(webNodeStore.getState().offers)).toEqual(["b"]);
    expect(webNodeStore.getState().projections.a.phase).toBe("terminal");
  });

  it("非终态的 projection 不动 offers", () => {
    webNodeActions.applyEvent(offerEvent("a"));
    webNodeActions.applyEvent(projectionEvent("a", "active"));

    expect(Object.keys(webNodeStore.getState().offers)).toEqual(["a"]);
  });

  // 「内容没变就不换引用」是本 store 的通用纪律（zustand 判 `Object.is(partial, state)`，
  // 返回 `{}` 会白广播一轮）。终态会话没有对应 offer 时不该凭空造一个新的 offers 对象。
  it("终态会话没有对应 offer 时，offers 保持同一引用", () => {
    const before = webNodeStore.getState().offers;
    webNodeActions.applyEvent(projectionEvent("ghost", "terminal"));

    expect(webNodeStore.getState().offers).toBe(before);
  });
});

// 用例里凡是要看到条目落域的，都得先建一条 active 的 projection：发布只发生在传输过程中，
// 而揭示回调拒绝给一条不活跃（或不在册）的会话写条目——那是「凭空长出一条永不消失的正在
// 保存」的唯一入口（`started` 因事件乱序落在终态之后，阈值到点时定时器仍会把它写回，而该
// 会话再也不会有新的 projection 来清它）。
describe("正在保存（暂存 → 发布）", () => {
  it("started 建条目、finished 摘掉", () => {
    webNodeActions.applyEvent(projectionEvent("a", "active"));
    webNodeActions.applyEvent(publishEvent("a", "started", "big.iso"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);
    expect(webNodeStore.getState().publishingBySession.a?.name).toBe("big.iso");

    webNodeActions.applyEvent(publishEvent("a", "finished", "big.iso"));
    expect(webNodeStore.getState().publishingBySession.a).toBeUndefined();
  });

  // 常数时间的发布（浏览器这边是 OPFS `close()`）里 started 与 finished 背靠背到达，却是
  // 两条独立事件、两次渲染——立刻落域就会让进度条每收齐一个文件闪一下灰，收一个几百个
  // 小文件的目录时是持续频闪。
  it("撑不过延迟揭示阈值的发布从不落域", () => {
    webNodeActions.applyEvent(publishEvent("a", "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS - 1);
    expect(webNodeStore.getState().publishingBySession.a).toBeUndefined();

    webNodeActions.applyEvent(publishEvent("a", "finished"));
    // 揭示定时器必须一并取消：漏了它，这里会在阈值到点时凭空写回一条永远没人清的
    // 「正在保存」。
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);
    expect(webNodeStore.getState().publishingBySession.a).toBeUndefined();
  });

  // 同一条取消路径的另一半：会话先一步转了终态（发布失败以可恢复的中断冒泡，走的是状态
  // 转换那条路），在途的揭示定时器同样不能在之后醒来。
  it("会话转入非 active 后，在途的揭示不再落域", () => {
    webNodeActions.applyEvent(publishEvent("a", "started"));
    webNodeActions.applyEvent(projectionEvent("a", "suspended"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);

    expect(webNodeStore.getState().publishingBySession.a).toBeUndefined();
  });

  // 这条是「正在保存」不会永久挂住的唯一保证：`finished` 只在发布**成功**时到达，
  // 失败会以可恢复的中断冒泡，走的是会话状态转换那条路。少了这条清理，一次失败的接收
  // 会永远顶着一句「正在保存 x.zip」。
  it("会话转入非 active 时清掉已揭示的发布", () => {
    webNodeActions.applyEvent(projectionEvent("a", "active"));
    webNodeActions.applyEvent(publishEvent("a", "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);
    expect(webNodeStore.getState().publishingBySession.a).toBeDefined();

    webNodeActions.applyEvent(projectionEvent("a", "suspended"));

    expect(webNodeStore.getState().publishingBySession.a).toBeUndefined();
  });

  it("active 的 projection 不动它", () => {
    webNodeActions.applyEvent(publishEvent("a", "started"));
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);
    const before = webNodeStore.getState().publishingBySession;
    webNodeActions.applyEvent(projectionEvent("a", "active"));

    expect(webNodeStore.getState().publishingBySession).toBe(before);
  });

  // 同 offers 那条纪律：内容没变就不换引用。会话先一步转了终态时，`finished` 会晚一步
  // 到达一个已经清空的条目——那时不该凭空造一个新对象出来白广播一轮。
  it("finished 对不存在的条目是无操作，不换引用", () => {
    const before = webNodeStore.getState().publishingBySession;
    webNodeActions.applyEvent(publishEvent("ghost", "finished"));

    expect(webNodeStore.getState().publishingBySession).toBe(before);
  });
});

// 定时器的清理出口共四个：会话转非 active、发布 `finished`、单条记录删除、store reset。
// 第五个是**成批**删记录——本端此前缺它（桌面与移动都有对应出口）。台账的不变量是
// 「每条在途定时器都对应一条在册的会话」，少一个出口就会留下一条醒来后往已经不存在的
// 会话上写「正在保存」的孤儿。
describe("记录被删时的定时器清理", () => {
  it("删单条记录后，在途的揭示不再落域", () => {
    webNodeActions.applyEvent(publishEvent("a", "started"));
    webNodeActions.removeProjection("a");
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);

    expect(webNodeStore.getState().publishingBySession.a).toBeUndefined();
  });

  it("清空已结束的记录后，孤儿定时器不再落域", () => {
    // 这条会话在册且是 terminal（`clearTerminalProjections` 的清理对象），但它的揭示
    // 定时器**没有**走过状态转换那条清理——`applyEvent(projection)` 先于 publish 到达。
    webNodeActions.applyEvent(projectionEvent("a", "terminal"));
    webNodeActions.applyEvent(publishEvent("a", "started"));
    expect(webNodeStore.getState().projections.a).toBeDefined();

    webNodeActions.clearTerminalProjections();
    vi.advanceTimersByTime(PUBLISH_VISIBLE_AFTER_MS);

    expect(webNodeStore.getState().publishingBySession.a).toBeUndefined();
  });

  it("清空已结束的记录不动还在传的会话的定时器", () => {
    webNodeActions.applyEvent(projectionEvent("live", "active"));
    webNodeActions.applyEvent(progressEvent("live"));
    webNodeActions.applyEvent(projectionEvent("done", "terminal"));

    webNodeActions.clearTerminalProjections();

    // 保鲜期定时器还在：到点了才作废，不是被批量清理顺手带走的。
    expect(webNodeStore.getState().progressAt.live).toBeDefined();
    vi.advanceTimersByTime(PROGRESS_STALE_MS);
    expect(webNodeStore.getState().progressAt.live).toBeUndefined();
  });
});

describe("进度帧的保鲜期", () => {
  it("每帧都记下到达时刻", () => {
    webNodeActions.applyEvent(progressEvent("a"));

    expect(webNodeStore.getState().progressAt.a).toBe(Date.now());
  });

  // 这条是整件事的要害：停滞时**没有任何新事件**，也就没有重渲染——光记到达时刻、渲染前
  // 判一下是不够的，界面会永远停在最后一帧画好的样子上，一个早已不成立的「剩余 45s」
  // 挂到会话超时。定时器到点的这次 setState 就是让订阅者重算的那一下。
  it("没有后续事件时，保鲜期到点自己作废", () => {
    webNodeActions.applyEvent(progressEvent("a"));
    expect(webNodeStore.getState().progressAt.a).toBeDefined();

    vi.advanceTimersByTime(PROGRESS_STALE_MS - 1);
    expect(webNodeStore.getState().progressAt.a).toBeDefined();

    vi.advanceTimersByTime(1);
    expect(webNodeStore.getState().progressAt.a).toBeUndefined();
    // 帧本身留着：字节数与百分比没有保质期，作废整帧会让进度条倒退回 projection 上那个
    // 只在状态转换时更新的值。
    expect(webNodeStore.getState().progress.a).toBeDefined();
  });

  it("新帧重置保鲜期", () => {
    webNodeActions.applyEvent(progressEvent("a"));
    vi.advanceTimersByTime(PROGRESS_STALE_MS - 1);
    webNodeActions.applyEvent(progressEvent("a", 20));

    // 若定时器没被重置，这一刻旧的那个就会醒来把新帧的到达时刻一起抹掉。
    vi.advanceTimersByTime(1);
    expect(webNodeStore.getState().progressAt.a).toBeDefined();

    vi.advanceTimersByTime(PROGRESS_STALE_MS);
    expect(webNodeStore.getState().progressAt.a).toBeUndefined();
  });

  it("会话转入非 active 时立刻作废，且定时器不再回写", () => {
    webNodeActions.applyEvent(progressEvent("a"));
    webNodeActions.applyEvent(projectionEvent("a", "terminal"));
    expect(webNodeStore.getState().progressAt.a).toBeUndefined();

    // 同 offers/publishing 那条纪律：没有条目可清时不换引用，否则白广播一轮。
    const before = webNodeStore.getState();
    vi.advanceTimersByTime(PROGRESS_STALE_MS);
    expect(webNodeStore.getState()).toBe(before);
  });
});

describe("发送准备进度", () => {
  it("首条事件自我认领活跃批次", () => {
    webNodeActions.applyEvent(prepareEvent("p1"));

    expect(webNodeStore.getState().activePrepare?.preparedId).toBe("p1");
  });

  // `send_files()` 内部生成 preparedId、不回传给调用方，所以认领只能靠事件自己。
  // 未完成的批次不让位，否则并发发送会让进度条在两批之间来回跳。
  it("未完成的批次不被后来者顶掉", () => {
    webNodeActions.applyEvent(prepareEvent("p1", 10));
    webNodeActions.applyEvent(prepareEvent("p2", 20));

    expect(webNodeStore.getState().activePrepare?.preparedId).toBe("p1");
    expect(webNodeStore.getState().activePrepare?.bytesHashed).toBe(10);
  });

  // 回归：MCP 工具（桌面）与任何异常退出的调用方都不会调 clearPrepare，于是一条跑到
  // 100% 的批次会把活跃位**永久**占住，此后每次发送的进度都被挡在门外。
  it("已跑到 100% 的批次会让位给新批次", () => {
    webNodeActions.applyEvent(prepareEvent("p1", 100));
    webNodeActions.applyEvent(prepareEvent("p2", 5));

    expect(webNodeStore.getState().activePrepare?.preparedId).toBe("p2");
  });

  it("同一批次的后续事件正常更新", () => {
    webNodeActions.applyEvent(prepareEvent("p1", 10));
    webNodeActions.applyEvent(prepareEvent("p1", 60));

    expect(webNodeStore.getState().activePrepare?.bytesHashed).toBe(60);
  });

  it("清理后下一批可以重新认领", () => {
    webNodeActions.applyEvent(prepareEvent("p1", 10));
    webNodeActions.clearPrepare();
    expect(webNodeStore.getState().activePrepare).toBeNull();

    webNodeActions.applyEvent(prepareEvent("p2", 5));
    expect(webNodeStore.getState().activePrepare?.preparedId).toBe("p2");
  });

  // 同 offers 那条纪律：内容没变就不换引用，否则白广播一轮。
  it("没有活跃批次时清理是无操作，state 保持同一引用", () => {
    const before = webNodeStore.getState();
    webNodeActions.clearPrepare();

    expect(webNodeStore.getState()).toBe(before);
  });

  it("被挡下的事件不换 activePrepare 引用", () => {
    webNodeActions.applyEvent(prepareEvent("p1", 10));
    const before = webNodeStore.getState().activePrepare;
    webNodeActions.applyEvent(prepareEvent("p2", 20));

    expect(webNodeStore.getState().activePrepare).toBe(before);
  });
});
