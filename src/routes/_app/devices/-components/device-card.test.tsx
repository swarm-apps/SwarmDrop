import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Device } from "@/lib/bindings";
import { DeviceCard } from "./device-card";

const offlineDevice: Device = {
  peerId: "12D3KooW123456",
  name: "Remote Mac",
  hostname: "macbook-pro",
  os: "macOS",
  platform: "darwin",
  arch: "arm64",
  capabilities: [],
  status: "offline",
  connection: null,
  connectionDetails: null,
  latency: null,
  isPaired: true,
  trustLevel: "collaborator",
  receivePolicy: null,
  trustConfirmed: true,
};

afterEach(cleanup);

describe("DeviceCard organization display", () => {
  it("keeps a readable alias and group identity for an offline paired device", async () => {
    const user = userEvent.setup();
    const onOrganize = vi.fn();
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard
          device={offlineDevice}
          displayName="张三的 Mac"
          groupNames={["张三", "工作"]}
          identityHint="macbook-pro · 12D3…123456"
          showIdentityHint
          onOrganize={onOrganize}
          onUnpair={vi.fn()}
        />
      </I18nProvider>,
    );

    expect(screen.getByText("张三的 Mac")).toBeTruthy();
    expect(screen.getByText("张三 · 工作 · macbook-pro · 12D3…123456")).toBeTruthy();
    expect(screen.getByText("离线")).toBeTruthy();
    expect(screen.getByTestId("device-send-action").hasAttribute("disabled")).toBe(true);

    await user.click(screen.getByTestId("device-actions-menu"));
    await user.click(screen.getByTestId("device-organize-menu-action"));
    expect(onOrganize).toHaveBeenCalledWith(offlineDevice);
  });
});

describe("DeviceCard connection badge", () => {
  const onlineDevice: Device = {
    ...offlineDevice,
    status: "online",
    connection: "relay",
    connectionDetails: {
      transport: "quic",
      remoteAddr:
        "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWRelay/p2p-circuit/p2p/12D3KooW123456",
      relay: "12D3KooWRelay",
    },
    latency: null,
  };

  // 延迟要等第一次 ping（30s 间隔）。此前徽标的渲染条件把 latency 并了进去，
  // 于是刚连上的半分钟里连接方式完全不显示——回归会静默重现，钉在这里。
  it("shows the connection badge before the first latency sample", () => {
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard device={onlineDevice} displayName="Remote Mac" />
      </I18nProvider>,
    );

    const badge = screen.getByTestId("connection-badge");
    expect(badge.textContent).toContain("中继");
    expect(badge.textContent).toContain("QUIC");
  });

  it("degrades to a plain badge when the kernel has not reported a link yet", () => {
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard
          device={{ ...onlineDevice, connectionDetails: null }}
          displayName="Remote Mac"
        />
      </I18nProvider>,
    );

    // 点开是空的 popover 比没有这个入口更糟：详情缺席时徽标不该是个按钮
    expect(screen.queryByTestId("connection-badge")).toBeNull();
    expect(screen.getByText("中继")).toBeTruthy();
  });
});
