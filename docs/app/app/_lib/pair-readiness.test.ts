import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PAIRING_READINESS_TIMEOUT_MS, waitForPairingReadiness } from "./pair-readiness";
import { webNodeActions } from "./store";
import type { InfraLink } from "./view-types";

/**
 * 一条 relay 关系；`relay.kind === "active"` 即 `selectReservation` 认的「可拨中继」。
 *
 * **不做 `as unknown as` 强转**：那会让这个 fixture 与真实的 `InfraLink` 脱钩，而它存在的
 * 全部意义就是替 `selectReservation` 复现真实形状 —— 字段一旦改名，脱钩的 fixture 会让
 * 测试继续绿着，正好放过它该抓的那类回归。
 */
function link(active: boolean): InfraLink {
  return {
    peerId: "12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
    addrs: [],
    sources: [],
    roles: { kadServer: false, relayServer: true },
    scope: "public",
    firstSeen: "2026-08-12T00:00:00Z",
    lastSeen: "2026-08-12T00:00:00Z",
    removable: false,
    connected: active,
    relay: active
      ? { kind: "active", circuitAddr: "/ip4/1.2.3.4/tcp/4001/p2p-circuit" }
      : { kind: "connecting" },
    everActive: active,
    excluded: null,
  };
}

describe("waitForPairingReadiness", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    webNodeActions.reset();
    webNodeActions.markRunning("self");
  });
  afterEach(() => {
    vi.useRealTimers();
    webNodeActions.reset();
  });

  it("已经有 reservation 时立刻就绪，不等一帧", async () => {
    webNodeActions.setInfraLinks([link(true)]);
    // 完全不推进定时器就应当兑现——最常见的路径（节点早就跑着）不该被这道闸拖慢。
    await expect(waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS)).resolves.toBe(true);
  });

  it("reservation 建成的那一刻兑现", async () => {
    const pending = waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS);
    webNodeActions.setInfraLinks([link(false)]);
    await vi.advanceTimersByTimeAsync(3_000);
    webNodeActions.setInfraLinks([link(true)]);
    await expect(pending).resolves.toBe(true);
  });

  /**
   * 超时返回 `false` 而不是 reject —— 调用方据此**继续握手**（同网的 webrtc-direct
   * 地址与中继无关）。做成 reject 就等于让一个纯优化的时机调整掐死原本能成的配对。
   */
  it("等不到就放行，不抛错", async () => {
    const pending = waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS);
    await vi.advanceTimersByTimeAsync(PAIRING_READINESS_TIMEOUT_MS + 1);
    await expect(pending).resolves.toBe(false);
  });

  /**
   * 节点被停掉是「不会好了」而不是「还没好」。不认这条的话，用户在等待期间按下停止后
   * 还要对着一颗写着「正在连接中继…」的按钮再干等二十秒。
   */
  it("节点停掉立刻放行，不空等到超时", async () => {
    const pending = waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS);
    webNodeActions.setStatus("closing");
    await expect(pending).resolves.toBe(false);
  });

  /**
   * abort 后**必须立刻兑现，并且 `signal.aborted` 为真** —— 调用方正是靠后者区分
   * 「别等了，直接试」与「别试了」。分不开的后果不可逆：用户点了取消，那条一次性邀请
   * 却在二十秒后被照常消费掉。
   */
  it("被 abort 时立刻兑现，且调用方分得出这是取消", async () => {
    const abort = new AbortController();
    const pending = waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS, abort.signal);
    abort.abort();
    await expect(pending).resolves.toBe(false);
    expect(abort.signal.aborted).toBe(true);
  });

  it("传入一个已经 abort 的 signal 也不会挂住", async () => {
    await expect(
      waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS, AbortSignal.abort()),
    ).resolves.toBe(false);
  });

  /**
   * 进场时节点就已经停了 —— 与「等待途中被停掉」是**两条不同的路径**：订阅只在变化时
   * 触发，而这种情况下可能再也不会有下一次 setState。少了进场那一判，这里会坐满二十秒。
   *
   * 上面那条「节点停掉立刻放行」抓不到它：`beforeEach` 把状态置成了 running。
   */
  it("进场时节点就没在跑，立刻放行", async () => {
    webNodeActions.setStatus("error");
    await expect(waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS)).resolves.toBe(false);
  });
});
