import { I18nProvider } from "@lingui/react";
import { i18n } from "@lingui/core";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

vi.mock("@/lib/bindings", () => ({
  commands: {
    shutdown: vi.fn(),
    start: vi.fn(),
    listDevices: vi.fn(),
    getNetworkStatus: vi.fn(),
    startMcpServer: vi.fn(),
  },
  events: {
    devicesChanged: { listen: vi.fn(async () => vi.fn()) },
    networkStatusChanged: { listen: vi.fn(async () => vi.fn()) },
    pairingRequestReceived: { listen: vi.fn(async () => vi.fn()) },
    pairedDeviceAdded: { listen: vi.fn(async () => vi.fn()) },
  },
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

import { NetworkSettingsSection } from "./-network-settings-section";
import { useNetworkStore } from "@/stores/network-store";
import { usePreferencesStore } from "@/stores/preferences-store";

describe("NetworkSettingsSection", () => {
  const stopNetwork = vi.fn().mockResolvedValue(undefined);
  const startNetwork = vi.fn().mockResolvedValue(true);

  beforeEach(() => {
    vi.clearAllMocks();
    usePreferencesStore.setState({
      autoStart: false,
      discoveryMode: "auto",
      autoDiscoverLanHelpers: true,
      provideLanHelper: false,
    });
    useNetworkStore.setState({
      status: "running",
      stopNetwork,
      startNetwork,
    });
  });

  it("运行中修改局域网协助设置后显示重启提示并复用 stop/start", async () => {
    const user = userEvent.setup();
    render(
      <I18nProvider i18n={i18n}>
        <NetworkSettingsSection />
      </I18nProvider>,
    );

    await user.click(screen.getByRole("switch", { name: "本设备作为局域网协助节点" }));

    expect(usePreferencesStore.getState().provideLanHelper).toBe(true);
    expect(screen.queryByText("网络发现设置已变更，需重启节点生效")).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "重启节点" }));

    expect(stopNetwork).toHaveBeenCalledTimes(1);
    expect(startNetwork).toHaveBeenCalledTimes(1);
  });
});
