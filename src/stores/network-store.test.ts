import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  start: vi.fn().mockResolvedValue(null),
  shutdown: vi.fn().mockResolvedValue(null),
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock("@/lib/bindings", () => ({
  commands: {
    start: mocks.start,
    shutdown: mocks.shutdown,
    listDevices: vi.fn(),
    getNetworkStatus: vi.fn(),
    startMcpServer: vi.fn(),
    initializeIdentity: vi.fn(),
  },
  events: {
    devicesChanged: { listen: mocks.listen },
    networkStatusChanged: { listen: mocks.listen },
    pairingRequestReceived: { listen: mocks.listen },
    pairedDeviceAdded: { listen: mocks.listen },
  },
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

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

import {
  autoStartNodeIfEnabled,
  useNetworkStore,
} from "@/stores/network-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useSecretStore } from "@/stores/secret-store";

describe("network-store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.setState({
      status: "stopped",
      devices: [],
      networkStatus: null,
      error: null,
      startedAt: null,
    });
    useSecretStore.setState({
      deviceId: "12D3KooWLocal",
      pairedDevices: [
        {
          peerId: "12D3KooWPeer",
          hostname: "peer",
          os: "windows",
          platform: "windows",
          arch: "x86_64",
          pairedAt: 1,
        },
      ],
      initError: null,
    });
    usePreferencesStore.setState({
      customBootstrapNodes: ["/ip4/192.168.1.10/tcp/4001/p2p/12D3KooWBootstrap"],
      autoDiscoverLanHelpers: false,
      provideLanHelper: true,
      mcp: { port: 19527, autoStart: false },
    });
  });

  it("启动节点时把网络发现配置传给后端 start 命令", async () => {
    const ok = await useNetworkStore.getState().startNetwork();

    expect(ok).toBe(true);
    // 已配对设备不再作为实参传入：后端从 PairedDeviceStore 端口自取。
    expect(mocks.start).toHaveBeenCalledWith({
      bootstrapNodes: [
        "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
        "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
        "/ip4/192.168.1.10/tcp/4001/p2p/12D3KooWBootstrap",
      ],
      autoDiscoverLanHelpers: false,
      provideLanHelper: true,
      publicReachability: true,
    });
  });

  describe("冷启动自动启动", () => {
    it("开关关闭时不启动节点", async () => {
      usePreferencesStore.setState({ autoStart: false });

      await autoStartNodeIfEnabled();

      expect(mocks.start).not.toHaveBeenCalled();
      expect(useNetworkStore.getState().status).toBe("stopped");
    });

    it("开关打开时启动一次", async () => {
      usePreferencesStore.setState({ autoStart: true });

      await autoStartNodeIfEnabled();

      expect(mocks.start).toHaveBeenCalledTimes(1);
    });

    // 回归锚点：自动启动此前是 `_app.tsx` 里一个依赖 `networkStatus` 的 effect，
    // `stopNetwork()` 把状态置为 stopped 会立刻把它触发一遍——开关打开时用户根本停不掉
    // 节点。判据是「停止之后没有任何东西再启动它」，所以断言的是调用次数没涨。
    it("停止节点后不被自动重新启动", async () => {
      usePreferencesStore.setState({ autoStart: true });
      await autoStartNodeIfEnabled();
      // start 命令只把节点拉起来，running 由后端 networkStatusChanged 事件驱动，
      // 这里直接置为运行中以满足 stopNetwork 的前置条件。
      useNetworkStore.setState({ status: "running" });

      await useNetworkStore.getState().stopNetwork();

      expect(useNetworkStore.getState().status).toBe("stopped");
      expect(mocks.start).toHaveBeenCalledTimes(1);
    });

    // 上面这些锁的是 store 的行为，锁不住**回归源**——收敛环长在组件的 effect 里，
    // 任何 store 层测试都观测不到它。那条规则是跨文件的架构约束，落点在
    // `scripts/check-node-lifecycle.mjs`（`pnpm check:node-lifecycle`），与
    // check:clipboard / check:zustand-access 同一套机制。
    //
    // 这里曾放过一条 `readFileSync("src/routes/_app.tsx")` + 正则的断言。它只扫那**一个**
    // 文件——同一个环长在 `__root.tsx` 或任意组件里都照绿，而注释写得像已经钉住了回归源。
    // 部分覆盖的护栏比没有护栏更糟：它会让人以为这件事已经有人管了。
  });
});
