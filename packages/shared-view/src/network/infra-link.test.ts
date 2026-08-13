import { describe, expect, it } from "vitest";

import {
  INFRA_GRACE_MS,
  deriveInfraLinkState,
  type InfraLinkState,
} from "./infra-link";
import type { InfraLinkView } from "./types";

const NOW = 1_800_000_000_000;

function link(overrides: Partial<InfraLinkView> = {}): InfraLinkView {
  return {
    roles: { relayServer: true, kadServer: true },
    scope: "public",
    firstSeenMs: NOW - 60_000,
    connected: true,
    relay: null,
    everActive: false,
    excluded: null,
    ...overrides,
  };
}

const stateOf = (l: InfraLinkView, now = NOW): InfraLinkState =>
  deriveInfraLinkState(l, now).state;

describe("deriveInfraLinkState", () => {
  it("宽限只在三条件同时成立时生效", () => {
    const failed = { kind: "failed", lastError: "dial timeout" } as const;

    // 三条件齐 → 报失败
    expect(stateOf(link({ relay: failed }))).toBe("unreachable");

    // 缺「过宽限」：刚进表
    expect(stateOf(link({ relay: failed, firstSeenMs: NOW - 1_000 }))).toBe(
      "settling",
    );
    // 边界：恰好到 GRACE 即生效（>= 而非 >）
    expect(
      stateOf(link({ relay: failed, firstSeenMs: NOW - INFRA_GRACE_MS })),
    ).toBe("unreachable");
    expect(
      stateOf(link({ relay: failed, firstSeenMs: NOW - INFRA_GRACE_MS + 1 })),
    ).toBe("settling");

    // 缺「已见 Failed」：只是还在连——native 拨号超时 30s 远大于宽限，
    // 纯定时器会在首次拨号还在飞的时候就宣布失败
    expect(stateOf(link({ relay: { kind: "connecting" } }))).toBe("settling");
    expect(stateOf(link({ relay: null }))).toBe("settling");

    // 缺「!everActive」：见下一条
  });

  it("Settling 不返回成功档", () => {
    for (const relay of [null, { kind: "connecting" } as const]) {
      const p = deriveInfraLinkState(link({ relay }), NOW);
      expect(p.state).toBe("settling");
      expect(p.tone).toBe("neutral");
    }
  });

  it("everActive 的 link 掉线不吃宽限，立刻警示", () => {
    const justSeen = { everActive: true, firstSeenMs: NOW - 1_000 };

    // 曾经连上过 → 无论多新、无论有没有 failed，掉出 Active 就是 lost
    expect(stateOf(link({ ...justSeen, relay: null }))).toBe("lost");
    expect(stateOf(link({ ...justSeen, relay: { kind: "connecting" } }))).toBe(
      "lost",
    );
    expect(
      stateOf(
        link({ ...justSeen, relay: { kind: "failed", lastError: "reset" } }),
      ),
    ).toBe("lost");
    expect(deriveInfraLinkState(link({ ...justSeen }), NOW).tone).toBe(
      "warning",
    );

    // 恢复即回成功档
    expect(
      stateOf(
        link({
          ...justSeen,
          relay: { kind: "active", circuitAddr: "/ip4/1.2.3.4/tcp/4001" },
        }),
      ),
    ).toBe("ok");
  });

  it("excluded 是中性档，且盖过任何故障态", () => {
    const p = deriveInfraLinkState(
      link({
        excluded: { kind: "publicReachabilityDisabled" },
        relay: { kind: "failed", lastError: "should not surface" },
        firstSeenMs: NOW - 600_000,
      }),
      NOW,
    );
    expect(p.state).toBe("excluded");
    expect(p.tone).toBe("neutral");
    // 「你自己关的」不该渲染成「坏了」，也不该把错误原文摆出来
    expect(p.detail).toBeNull();

    // 未知判别码同样按「被设置排除」处理，不崩到故障档
    expect(stateOf(link({ excluded: { kind: "somethingNewer" } }))).toBe(
      "excluded",
    );
  });

  it("seedOnly 没有失败态", () => {
    const p = deriveInfraLinkState(
      link({
        roles: { relayServer: false, kadServer: true },
        relay: null,
        firstSeenMs: NOW - 600_000,
      }),
      NOW,
    );
    expect(p.state).toBe("seedOnly");
    expect(p.tone).toBe("neutral");
    expect(p.detail).toBeNull();
  });

  it("Active 即成功，failed 原样带出 lastError", () => {
    expect(
      stateOf(
        link({ relay: { kind: "active", circuitAddr: "/ip4/1.1.1.1/tcp/1" } }),
      ),
    ).toBe("ok");

    const p = deriveInfraLinkState(
      link({ relay: { kind: "failed", lastError: "Transport(Timeout)" } }),
      NOW,
    );
    expect(p.detail).toBe("Transport(Timeout)");
  });
});
