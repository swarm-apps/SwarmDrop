import { describe, expect, it } from "vitest";
import { summarizeNodeHealth, type InfraLinkView } from "@swarmdrop/shared-view";

import {
  INFRA_LINK_STATE_LABEL,
  NODE_HEALTH_MESSAGE,
  NODE_HEALTH_WORD,
  TONE_DOT,
  infraAddrErrorLabel,
  toInfraAddrError,
  toInfraLinkView,
  toNodeStatusView,
} from "./network-view";
import { infraNodesToReplay } from "./preferences-store";
import { WEB_RELAY_HELPERS, bootstrapPeerId } from "./relay-helpers";
import type { InfraAddrError, InfraLink } from "./view-types";

const NOW = 1_800_000_000_000;

/** 六种校验失败各一个样本，文案与识别两组测试共用。 */
const ADDR_ERRORS: InfraAddrError[] = [
  { kind: "malformed", detail: "invalid protocol" },
  { kind: "missingPeerId" },
  { kind: "noTransport" },
  { kind: "unsupportedTransport", transport: "tcp", supported: ["webrtcDirect"] },
  { kind: "selfAddr" },
  { kind: "duplicate" },
];

function link(overrides: Partial<InfraLink> = {}): InfraLink {
  return {
    peerId: "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN",
    addrs: ["/ip4/1.2.3.4/udp/4003/webrtc-direct/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"],
    sources: ["hostConfigured"],
    roles: { kadServer: true, relayServer: true },
    scope: "public",
    firstSeen: new Date(NOW - 600_000).toISOString(),
    lastSeen: new Date(NOW).toISOString(),
    removable: true,
    connected: false,
    relay: null,
    everActive: false,
    excluded: null,
    ...overrides,
  };
}

describe("toInfraLinkView", () => {
  it("ISO 串转成毫秒（跨 wasm 边界的时间都是 ISO，判据要的是数字）", () => {
    const view = toInfraLinkView(link(), NOW);
    expect(view.firstSeenMs).toBe(NOW - 600_000);
  });

  it("时间解析不出来时退回「现在」，而不是 0", () => {
    // 退回 0 会让这条 link 一进表就判定「过了宽限」，首次拨号还在飞就被宣布连不上。
    const view = toInfraLinkView(link({ firstSeen: "不是时间" }), NOW);
    expect(view.firstSeenMs).toBe(NOW);
  });
});

describe("toNodeStatusView", () => {
  it("浏览器的公网可达性 = 有没有一条活的 circuit 预留", () => {
    expect(toNodeStatusView("running", null, 0).publicReachable).toBe(false);
    expect(toNodeStatusView("running", "/ip4/1.1.1.1/tcp/1/p2p-circuit", 0).publicReachable).toBe(
      true,
    );
  });

  it("只有 running 一档算「在跑」", () => {
    for (const status of ["idle", "starting", "closing", "error"] as const) {
      expect(toNodeStatusView(status, null, 0).status).toBe("stopped");
    }
    expect(toNodeStatusView("running", null, 0).status).toBe("running");
  });
});

/**
 * 这一组钉的是本轮要修的那个缺陷：**节点在跑、但全部中继都挂了时，常驻徽章仍然是绿的**。
 * 徽章的色档 = `TONE_DOT[summarizeNodeHealth(...).tone]`，所以在这一层断言就够。
 */
describe("常驻徽章的色档", () => {
  const failed = (): InfraLinkView =>
    toInfraLinkView(link({ relay: { kind: "failed", lastError: "dial timeout" } }), NOW);

  it("全部中继失败时不是成功色", () => {
    const summary = summarizeNodeHealth(
      toNodeStatusView("running", null, 0),
      [failed(), failed()],
      NOW,
    );
    expect(summary.level).toBe("isolated");
    expect(TONE_DOT[summary.tone]).not.toBe(TONE_DOT.success);
    expect(TONE_DOT[summary.tone]).toBe("bg-warning");
  });

  it("有一条 circuit 活着才是成功色", () => {
    const active = toInfraLinkView(
      link({ relay: { kind: "active", circuitAddr: "/ip4/1.1.1.1/tcp/1" }, everActive: true }),
      NOW,
    );
    const summary = summarizeNodeHealth(
      toNodeStatusView("running", "/ip4/1.1.1.1/tcp/1/p2p-circuit", 0),
      [active, failed()],
      NOW,
    );
    // 「部分节点连不上」不是降级——连上一条与连上两条的后果完全一样。
    expect(summary.level).toBe("reachable");
    expect(TONE_DOT[summary.tone]).toBe("bg-success");
  });
});

describe("契约文案", () => {
  it("六档健康度各有一条后果句与一个词，且互不相同", () => {
    const messages = new Set(Object.values(NODE_HEALTH_MESSAGE).map((m) => m.id ?? m.message));
    expect(messages.size).toBe(Object.keys(NODE_HEALTH_MESSAGE).length);
    // 词允许重复（lanReachable 与 configuredLanOnly 对用户就是同一句「仅局域网」，
    // 区别在后果句里说），所以只断言每档都有。
    for (const level of Object.keys(NODE_HEALTH_MESSAGE)) {
      expect(NODE_HEALTH_WORD[level as keyof typeof NODE_HEALTH_WORD]).toBeDefined();
    }
  });

  it("六档关系状态各有一条文案，且互不相同", () => {
    const labels = new Set(Object.values(INFRA_LINK_STATE_LABEL).map((m) => m.id ?? m.message));
    expect(labels.size).toBe(Object.keys(INFRA_LINK_STATE_LABEL).length);
  });

  it("六种校验失败各给一句能据以行动的话", () => {
    for (const error of ADDR_ERRORS) {
      expect(infraAddrErrorLabel(error)).toBeTruthy();
    }
  });
});

/**
 * `toInfraAddrError` 是 reject 值的**唯一入口**：认不出来就退回通用的「添加引导节点失败」，
 * 那句话丢掉了全部可行动信息。所以「认得出多少种」必须与 `infraAddrErrorLabel` 那个无
 * `default` 的 switch 一致——判别码表此前是个手写的 `Set`，core 加变体时 switch 会红、
 * 它却静默漏掉。现在两处同由枚举做键，这一组是运行时的第二道保险。
 */
describe("toInfraAddrError", () => {
  it("六种判别码全部认得，且原样返回", () => {
    for (const error of ADDR_ERRORS) {
      expect(toInfraAddrError(error)).toBe(error);
    }
  });

  it("`WebError` 的判别码不冒充校验失败", () => {
    // 两个枚举的取值刻意互不重叠——重叠了这里会静默走错分支。
    expect(toInfraAddrError({ kind: "network", message: "boom" })).toBeNull();
  });

  it("原型链上的键不算命中", () => {
    expect(toInfraAddrError({ kind: "toString" })).toBeNull();
    expect(toInfraAddrError({ kind: "constructor" })).toBeNull();
  });

  it("不是对象 / 没有 kind 的一律 null", () => {
    expect(toInfraAddrError(new Error("boom"))).toBeNull();
    expect(toInfraAddrError(null)).toBeNull();
    expect(toInfraAddrError("duplicate")).toBeNull();
  });
});

/**
 * 持久化存的是 **custom + removed 两个集合**，不是合并后的快照。这一组钉的正是那个差别：
 * 合并快照会在新版本更换内置地址时把老用户永久压在旧地址上，而故障形态是「升级后突然
 * 连不上」且无法自查。
 */
describe("infraNodesToReplay", () => {
  const builtinPeerId = bootstrapPeerId(WEB_RELAY_HELPERS[0]!)!;

  it("零配置时回放的就是当前版本的内置清单", () => {
    expect(infraNodesToReplay({ custom: [], removed: [] })).toEqual(WEB_RELAY_HELPERS);
  });

  it("撤销过的内置项刷新后不复活", () => {
    const replayed = infraNodesToReplay({ custom: [], removed: [builtinPeerId] });
    expect(replayed).not.toContain(WEB_RELAY_HELPERS[0]);
  });

  it("撤销记的是 peer id，所以内置地址换了新的仍然被撤销", () => {
    // 同一台机器换端口/加 certhash → peer id 不变 → 用户的撤销仍然生效。
    expect(
      infraNodesToReplay({ custom: [], removed: [builtinPeerId] }).some((addr) =>
        addr.includes(builtinPeerId),
      ),
    ).toBe(false);
    // 换成另一台机器（peer id 变了）→ 用户从没对它表过态 → 照常回放。
    expect(infraNodesToReplay({ custom: [], removed: ["12D3KooWSomeoneElse"] })).toEqual(
      WEB_RELAY_HELPERS,
    );
  });

  it("自定义项跟在内置项之后一并回放", () => {
    const custom = "/ip4/10.0.0.1/udp/4003/webrtc-direct/p2p/12D3KooWCustom";
    expect(infraNodesToReplay({ custom: [custom], removed: [] })).toEqual([
      ...WEB_RELAY_HELPERS,
      custom,
    ]);
  });
});
