/**
 * 节点状态面的**信息位不丢**回归测试。
 *
 * 这个面此前按 `windowHeight >= 700` 门控七处内容：矮窗口下中继状态、引导节点、
 * 公网地址、监听地址整片消失——而那恰恰是用户在排查「连不上」时唯一要看的东西，
 * 也正是 `DESIGN.md` 的 Node Status Contract 明令禁止的（「No build may drop a slot
 * because the layout is tight」）。所以这条测试把窗口压到 600px 再断言它们仍在。
 */

import { I18nProvider } from "@lingui/react";
import { i18n } from "@lingui/core";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-os", () => ({
  hostname: async () => "test-host",
  platform: () => "macos",
  type: () => "macos",
}));

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

vi.mock("@/lib/i18n", () => ({
  defaultLocale: "zh",
  dynamicActivate: vi.fn(),
  locales: { zh: "简体中文" },
}));

vi.mock("@/lib/clipboard", () => ({
  copyText: vi.fn(async () => {}),
  readText: vi.fn(async () => ""),
}));

// `vi.mock` 的工厂被 hoist 到文件顶部，引用不到普通的顶层常量——路径要经 `vi.hoisted`
// 才能同时被工厂和用例读到（仓库里其他 mock 用的是同一个模式）。
const { IDENTITY_PATH } = vi.hoisted(() => ({
  IDENTITY_PATH:
    "/Users/demo/Library/Application Support/com.yexiyue.swarmdrop/identity.json",
}));

vi.mock("@/lib/bindings", () => ({
  commands: {
    getIdentityFilePath: vi.fn().mockResolvedValue(IDENTITY_PATH),
  },
  events: {},
}));

vi.mock("sonner", () => ({
  toast: { error: vi.fn(), success: vi.fn() },
}));

// TanStack Router 的 Link 需要 router context，本测试只关心渲染出的信息位。
vi.mock("@tanstack/react-router", () => ({
  Link: ({
    children,
    ...rest
  }: React.PropsWithChildren<Record<string, unknown>>) => (
    <a {...(rest as Record<string, string>)}>{children}</a>
  ),
}));

import { NodeStatusSheet } from "./node-status-sheet";
import { useNetworkStore } from "@/stores/network-store";
import type { InfraLink, NetworkStatus } from "@/lib/bindings";

const PEER = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";

function link(overrides: Partial<InfraLink> = {}): InfraLink {
  return {
    peerId: PEER,
    addrs: ["/ip4/47.115.172.218/tcp/4001/p2p/" + PEER],
    sources: ["hostConfigured"],
    roles: { kadServer: true, relayServer: true },
    scope: "public",
    firstSeen: new Date(Date.now() - 60_000).toISOString(),
    lastSeen: new Date().toISOString(),
    removable: true,
    connected: true,
    relay: { kind: "failed", lastError: "dial timeout after 30s" },
    everActive: false,
    excluded: null,
    ...overrides,
  };
}

function networkStatus(links: InfraLink[]): NetworkStatus {
  return {
    status: "running",
    peerId: PEER,
    listenAddrs: ["/ip4/192.168.1.9/tcp/4001"],
    natStatus: "unknown",
    publicAddr: "/ip4/203.0.113.7/tcp/4001",
    connectedPeers: 0,
    discoveredPeers: 2,
    relayReady: false,
    publicReachable: false,
    publicReachabilityEnabled: true,
    relayPeers: [],
    bootstrapConnected: false,
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

describe("NodeStatusSheet", () => {
  // 根 vitest 没开 globals，`@testing-library/react` 的自动 cleanup 不会注册，
  // 不显式清理会让第二个用例在第一个用例的 DOM 上找到两份同名节点。
  afterEach(cleanup);

  beforeEach(() => {
    // 矮窗口——此前这个高度会让七处信息位整片消失。
    Object.defineProperty(window, "innerHeight", {
      writable: true,
      configurable: true,
      value: 600,
    });
    useNetworkStore.setState({
      status: "running",
      devices: [],
      networkStatus: networkStatus([link()]),
      startedAt: Date.now() - 120_000,
    });
  });

  it("矮窗口下诊断层的信息位一个都不少", () => {
    render(
      <I18nProvider i18n={i18n}>
        <NodeStatusSheet open onOpenChange={() => {}} />
      </I18nProvider>,
    );

    // 结论层：状态词 + 后果句（光有色点不满足契约）
    expect(screen.queryByTestId("node-status-conclusion")).not.toBeNull();
    expect(screen.queryByText("连不上任何网络，检查引导节点")).not.toBeNull();

    // 诊断层：逐条引导节点 + 原样 lastError
    expect(screen.queryByTestId("infra-links")).not.toBeNull();
    expect(screen.queryByText("dial timeout after 30s")).not.toBeNull();

    // 归因必须齐三项「来源 · 范围 · 角色」：只画来源时，手填的局域网节点与公网节点
    // 长得一模一样，而 scope 正是「你关了公网可达性」那一档的判据。
    expect(screen.queryByTestId("infra-link-attribution")?.textContent).toBe(
      "配置节点 · 公网 · DHT 种子 · 中继",
    );

    // 诊断层：本机真值
    expect(screen.queryByText("节点 ID")).not.toBeNull();
    expect(screen.queryByText("公网地址")).not.toBeNull();
    expect(screen.queryByText("NAT 状态")).not.toBeNull();
    expect(screen.queryByText("已发现节点")).not.toBeNull();
    expect(screen.queryByText("运行时长")).not.toBeNull();
    expect(screen.queryByText("监听地址")).not.toBeNull();
    // 身份存放位置也是 Node Status Contract 列明的信息位之一，此前这条护栏漏了它。
    expect(screen.queryByText("身份文件")).not.toBeNull();
  });

  // 光断言 label 渲染是不够的：`getIdentityFilePath` 失败时那一行会退成 `—`，label 照在，
  // 而 DESIGN.md 的契约要的是「可据以行动的路径」——「system keychain」和「一个本地文件」
  // 一样没用。所以要断言路径本身到达了 DOM。
  it("身份文件那行给的是可复制的完整路径，不只是一个标签", async () => {
    render(
      <I18nProvider i18n={i18n}>
        <NodeStatusSheet open onOpenChange={() => {}} />
      </I18nProvider>,
    );

    const row = await screen.findByTitle(IDENTITY_PATH);
    // 屏幕上是中间省略的短形式，复制/title 给完整值（见 CopyableValue 的文档）。
    expect(row.textContent).toContain("…");
    expect(row.textContent).toContain("identity.json");
  });

  it("节点没跑时给的是启动动作，不是四个写死的 0", () => {
    useNetworkStore.setState({ status: "stopped", networkStatus: null });
    render(
      <I18nProvider i18n={i18n}>
        <NodeStatusSheet open onOpenChange={() => {}} />
      </I18nProvider>,
    );

    expect(screen.queryByTestId("start-node-confirm-action")).not.toBeNull();
    expect(screen.queryByTestId("stop-node-confirm-action")).toBeNull();
    expect(screen.queryByText("节点未运行")).not.toBeNull();
  });
});
