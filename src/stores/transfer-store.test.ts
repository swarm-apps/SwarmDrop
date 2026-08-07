import { beforeEach, describe, expect, it, vi } from "vitest";

// store 模块顶层 import 了 bindings（Tauri IPC）与通知桥接，测试里都不该真跑。
vi.mock("@/lib/bindings", () => ({
  commands: { getTransferProjections: vi.fn(async () => []) },
  events: {
    transferProjectionUpdate: { listen: vi.fn() },
    transferOffer: { listen: vi.fn() },
    transferProgress: { listen: vi.fn() },
  },
}));
vi.mock("@/lib/transfer-notifications", () => ({
  setupTransferNotifications: vi.fn(async () => () => {}),
}));

import type { TransferOfferEvent, TransferProjection } from "@/lib/bindings";
import { useTransferStore } from "./transfer-store";

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

beforeEach(() => {
  useTransferStore.setState({
    projections: {},
    progressBySession: {},
    pendingOffers: [],
    dismissedOfferIds: [],
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
