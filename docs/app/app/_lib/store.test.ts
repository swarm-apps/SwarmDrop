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

beforeEach(() => {
  webNodeStore.setState({ offers: {}, projections: {}, eventLog: [] });
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
