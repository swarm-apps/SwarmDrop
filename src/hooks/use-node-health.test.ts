/**
 * 宽限时钟的**走表条件**回归测试。
 *
 * 这里曾用 `summary.level === "starting"` 当条件，两个方向都错，各钉一条：
 *
 * 1. 「两条引导节点连上一条」时整体是 `reachable`（`resolveLevel` 在 `publicReachable`
 *    为真时先于 settling 判定就 return 了），定时器不起 → `nowMs` 永远停在 mount 那一刻
 *    → 那条挂掉的 link 恒为「正在连接」，而它的 `lastError` 就印在同一张卡下面两行。
 * 2. 节点在跑但 `infraLinks` 为空时整体恒为 `starting`，定时器于是永不停止，顶栏 pill
 *    每 2 秒白重渲染一次。
 */

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/tauri-store", () => ({
  createTauriStorage: () => {
    const values = new Map<string, string>();
    return {
      getItem: async (key: string) => values.get(key) ?? null,
      setItem: async (key: string, value: string) => {
        values.set(key, value);
      },
      removeItem: async (key: string) => {
        values.delete(key);
      },
    };
  },
}));

vi.mock("@/lib/bindings", () => ({
  commands: {},
  events: {},
}));

import { useNodeHealth } from "./use-node-health";
import { deriveInfraLinkState, toInfraLinkView } from "@/lib/node-status";
import { useNetworkStore } from "@/stores/network-store";
import type { InfraLink, NetworkStatus } from "@/lib/bindings";

const PEER = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";

/** 一条刚进候选表、首拨已失败的 link——判据此刻应给 `settling`，过宽限后翻 `unreachable`。 */
function freshlyFailedLink(): InfraLink {
  return {
    peerId: PEER,
    addrs: [`/ip4/47.115.172.218/tcp/4001/p2p/${PEER}`],
    sources: ["hostConfigured"],
    roles: { kadServer: true, relayServer: true },
    scope: "public",
    firstSeen: new Date(Date.now()).toISOString(),
    lastSeen: new Date(Date.now()).toISOString(),
    removable: true,
    connected: false,
    relay: { kind: "failed", lastError: "dial timeout after 30s" },
    everActive: false,
    excluded: null,
  };
}

function networkStatus(
  links: InfraLink[],
  publicReachable: boolean,
): NetworkStatus {
  return {
    status: "running",
    peerId: PEER,
    listenAddrs: [],
    natStatus: "unknown",
    publicAddr: publicReachable ? "/ip4/203.0.113.7/tcp/4001" : null,
    connectedPeers: publicReachable ? 1 : 0,
    discoveredPeers: 0,
    relayReady: publicReachable,
    publicReachable,
    publicReachabilityEnabled: true,
    relayPeers: [],
    bootstrapConnected: publicReachable,
    autoDiscoverLanHelpers: true,
    localLanHelperEnabled: false,
    localLanHelperRunning: false,
    relayServerEnabled: false,
    lanHelperAdvertisedAddrs: [],
    lanHelperCount: 0,
    bootstrapCandidateCount: links.length,
    candidateSources: [],
    relaySource: null,
    infraLinks: links,
  };
}

describe("useNodeHealth 的宽限时钟", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("整体已 reachable，但仍有 link 在宽限内时照样走表", () => {
    // 一条连上（publicReachable=true）、另一条刚拨失败——正是 design.md 举的典型场景。
    useNetworkStore.setState({
      status: "running",
      networkStatus: networkStatus([freshlyFailedLink()], true),
    });

    const { result } = renderHook(() => useNodeHealth());
    expect(result.current.summary.level).toBe("reachable");
    const stateNow = () =>
      deriveInfraLinkState(
        toInfraLinkView(result.current.links[0]),
        result.current.nowMs,
      ).state;
    expect(stateNow()).toBe("settling");

    // 过了宽限窗口：没有任何后端推送（`send_if_modified` 已把 Failed 去重掉），
    // 翻转只能靠自己走表。
    act(() => {
      vi.advanceTimersByTime(12_000);
    });
    expect(stateNow()).toBe("unreachable");
  });

  it("没有 link 在收敛时不空转", () => {
    // 节点在跑但候选表是空的：整体恒为 `starting`，但没有任何待定判决要重算。
    useNetworkStore.setState({
      status: "running",
      networkStatus: networkStatus([], false),
    });

    const { result } = renderHook(() => useNodeHealth());
    expect(result.current.summary.level).toBe("starting");
    const mountedAt = result.current.nowMs;

    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(result.current.nowMs).toBe(mountedAt);
  });
});
