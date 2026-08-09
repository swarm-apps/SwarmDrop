import { beforeEach, describe, expect, it } from "vitest";
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

beforeEach(() => {
  webNodeStore.setState({
    offers: {},
    projections: {},
    eventLog: [],
    activePrepare: null,
    clearedPreparedId: null,
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
