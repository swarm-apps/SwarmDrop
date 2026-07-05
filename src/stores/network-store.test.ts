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

import { useNetworkStore } from "@/stores/network-store";
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
      discoveryMode: "lanOnly",
      autoDiscoverLanHelpers: false,
      provideLanHelper: true,
      mcp: { port: 19527, autoStart: false },
    });
  });

  it("启动节点时把网络发现配置传给后端 start 命令", async () => {
    const ok = await useNetworkStore.getState().startNetwork();

    expect(ok).toBe(true);
    expect(mocks.start).toHaveBeenCalledWith(
      useSecretStore.getState().pairedDevices,
      {
        customBootstrapNodes: [
          "/ip4/192.168.1.10/tcp/4001/p2p/12D3KooWBootstrap",
        ],
        discoveryMode: "lanOnly",
        autoDiscoverLanHelpers: false,
        provideLanHelper: true,
        publicReachability: true,
      },
    );
  });
});
