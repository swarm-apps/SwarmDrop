import { describe, expect, it } from "vitest";

import { summarizeNodeHealth, type NodeHealthLevel } from "./node-health";
import type { InfraLinkView, NodeStatusView } from "./types";

const NOW = 1_800_000_000_000;
const OLD = NOW - 600_000;

function status(overrides: Partial<NodeStatusView> = {}): NodeStatusView {
  return {
    status: "running",
    publicReachable: false,
    connectedPeers: 0,
    ...overrides,
  };
}

function link(overrides: Partial<InfraLinkView> = {}): InfraLinkView {
  return {
    roles: { relayServer: true, kadServer: true },
    scope: "public",
    firstSeenMs: OLD,
    connected: false,
    relay: null,
    everActive: false,
    excluded: null,
    ...overrides,
  };
}

const FAILED = { kind: "failed", lastError: "dial timeout" } as const;
const ACTIVE = { kind: "active", circuitAddr: "/ip4/1.2.3.4/tcp/4001" } as const;

const levelOf = (
  s: NodeStatusView,
  links: InfraLinkView[] = [],
): NodeHealthLevel => summarizeNodeHealth(s, links, NOW).level;

describe("summarizeNodeHealth", () => {
  it("节点没跑时说的是「节点未运行」，而不是归因到引导节点", () => {
    // 这正是移动端 NetworkHint 的既有缺陷：节点没起来却提示「公网引导节点未连接」，
    // 用户去查了一圈引导节点，而真正该点的是「启动节点」。
    const s = status({ status: "stopped" });
    const summary = summarizeNodeHealth(s, [link({ relay: FAILED })], NOW);
    expect(summary.level).toBe("notRunning");
    expect(summary.cta).toBe("startNode");
  });

  it("公网可达是成功档的唯一判据", () => {
    const summary = summarizeNodeHealth(
      status({ publicReachable: true }),
      // 另一条挂了也不降级——「部分节点连不上」不是降级
      [link({ relay: ACTIVE }), link({ relay: FAILED })],
      NOW,
    );
    expect(summary.level).toBe("reachable");
    expect(summary.tone).toBe("success");
    expect(summary.cta).toBeNull();
  });

  it("用户关掉公网可达性时是中性档 + 去设置，不是重试", () => {
    const excluded = { kind: "publicReachabilityDisabled" };
    const summary = summarizeNodeHealth(
      status(),
      [link({ excluded }), link({ excluded })],
      NOW,
    );
    expect(summary.level).toBe("configuredLanOnly");
    expect(summary.tone).toBe("neutral");
    expect(summary.cta).toBe("openSettings");
  });

  it("configuredLanOnly 压过 lanReachable —— 两句话说同一件事，只有它能指出去哪改", () => {
    expect(
      levelOf(status({ connectedPeers: 3 }), [
        link({ excluded: { kind: "publicReachabilityDisabled" } }),
      ]),
    ).toBe("configuredLanOnly");
  });

  it("只有部分 public link 被排除时不算 configuredLanOnly", () => {
    expect(
      levelOf(status({ connectedPeers: 1 }), [
        link({ excluded: { kind: "publicReachabilityDisabled" } }),
        link({ relay: FAILED }),
      ]),
    ).toBe("lanReachable");
  });

  it("局域网可达是中性档，不报警", () => {
    const summary = summarizeNodeHealth(
      status({ connectedPeers: 2 }),
      [link({ relay: FAILED })],
      NOW,
    );
    expect(summary.level).toBe("lanReachable");
    expect(summary.tone).toBe("neutral");
    expect(summary.cta).toBeNull();
  });

  it("还有 link 在收敛时安静地等，不提前报警", () => {
    expect(levelOf(status(), [link({ relay: { kind: "connecting" } })])).toBe(
      "starting",
    );
    // 刚进表、已经失败一次，但没过宽限 —— 仍算收敛中
    expect(
      levelOf(status(), [link({ relay: FAILED, firstSeenMs: NOW - 1_000 })]),
    ).toBe("starting");
  });

  it("空清单走 starting —— 喊「检查引导节点」是指着一张空表说话", () => {
    expect(levelOf(status(), [])).toBe("starting");
  });

  it("全挂且无对端才是孤立，且带诊断入口", () => {
    const summary = summarizeNodeHealth(
      status(),
      [link({ relay: FAILED }), link({ relay: FAILED })],
      NOW,
    );
    expect(summary.level).toBe("isolated");
    expect(summary.tone).toBe("warning");
    expect(summary.cta).toBe("openDiagnostics");
  });

  it("六态的 msgId 互不相同", () => {
    const cases: Array<[NodeHealthLevel, NodeStatusView, InfraLinkView[]]> = [
      ["notRunning", status({ status: "stopped" }), []],
      ["starting", status(), [link({ relay: { kind: "connecting" } })]],
      ["reachable", status({ publicReachable: true }), []],
      ["lanReachable", status({ connectedPeers: 1 }), [link({ relay: FAILED })]],
      [
        "configuredLanOnly",
        status(),
        [link({ excluded: { kind: "publicReachabilityDisabled" } })],
      ],
      ["isolated", status(), [link({ relay: FAILED })]],
    ];

    const seen = new Set<string>();
    for (const [expected, s, links] of cases) {
      const summary = summarizeNodeHealth(s, links, NOW);
      expect(summary.level).toBe(expected);
      seen.add(summary.msgId);
    }
    expect(seen.size).toBe(cases.length);
  });
});
