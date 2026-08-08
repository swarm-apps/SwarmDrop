import { I18nProvider } from "@lingui/react";
import { i18n } from "@lingui/core";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

const BANNER_TEXT = "网络发现设置已变更，需重启节点生效";

describe("NetworkSettingsSection", () => {
  const stopNetwork = vi.fn().mockResolvedValue(undefined);
  const startNetwork = vi.fn().mockResolvedValue(true);

  afterEach(cleanup);

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
      needsRestart: false,
      stopNetwork,
      startNetwork,
      setNeedsRestart: (needsRestart) => useNetworkStore.setState({ needsRestart }),
    });
  });

  it("运行中修改局域网协助设置后，重启提示就贴在那一行下面并复用 stop/start", async () => {
    const user = userEvent.setup();
    render(
      <I18nProvider i18n={i18n}>
        <NetworkSettingsSection />
      </I18nProvider>,
    );

    await user.click(screen.getByRole("switch", { name: "本设备作为局域网协助节点" }));

    expect(usePreferencesStore.getState().provideLanHelper).toBe(true);

    // 提示条**在设置卡里面**（贴着产生它的那些开关），不再吊在页面底部。
    const banner = screen.getByText(BANNER_TEXT);
    expect(banner.closest(".glass-card")).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "重启节点" }));

    expect(stopNetwork).toHaveBeenCalledTimes(1);
    expect(startNetwork).toHaveBeenCalledTimes(1);
  });

  /**
   * 「需重启」住在 store 而不是组件 `useState`：此前改完开关切去别的路由再回来，
   * 提示就没了，用户以为已经生效。这条测试用「卸载再挂载」模拟那次离开。
   */
  it("需重启的标记跨路由存活", async () => {
    const user = userEvent.setup();
    const first = render(
      <I18nProvider i18n={i18n}>
        <NetworkSettingsSection />
      </I18nProvider>,
    );

    await user.click(screen.getByRole("switch", { name: "本设备作为局域网协助节点" }));
    expect(screen.queryByText(BANNER_TEXT)).not.toBeNull();

    first.unmount();
    render(
      <I18nProvider i18n={i18n}>
        <NetworkSettingsSection />
      </I18nProvider>,
    );

    expect(screen.queryByText(BANNER_TEXT)).not.toBeNull();
  });
});
