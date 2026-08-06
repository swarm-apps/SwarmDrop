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
  lanUpgradeFailed: false,
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

    expect(screen.getByTestId("connection-badge").textContent).toContain("中继");
  });

  // 传输名（`WebRTC Direct` 这类）2026-08-06 起**不再印在徽标上**——它是四段里最长的
  // 一段，把网格里的窄列撑满，挤掉同一行的信任徽标。它退到悬停摘要 + Popover 里。
  //
  // 这条钉的是「短」这一半；另一半（它没有消失、点击仍可达）由下面那条钉。
  // 两条必须成对：只钉前者，有人把它整段删掉也是绿的。
  it("keeps the transport name out of the badge itself", () => {
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard device={onlineDevice} displayName="Remote Mac" />
      </I18nProvider>,
    );

    expect(screen.getByTestId("connection-badge").textContent).not.toContain("QUIC");
  });

  it("still exposes the transport on click, without hover", async () => {
    const user = userEvent.setup();
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard device={onlineDevice} displayName="Remote Mac" />
      </I18nProvider>,
    );

    // 点击——不是悬停。契约要求每个交互控件都能不靠 hover 到达，触屏与键盘走的是这条。
    await user.click(screen.getByTestId("connection-badge"));
    expect(await screen.findByText("QUIC")).toBeTruthy();
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

  // DESIGN.md 的 Device Card Contract 要求信息位 5（信任）与 6（连接）**同时在场**。
  // 此前这里是三元二选一，于是连上的设备永远看不到自己的信任级别——而那正是它最要紧的
  // 时刻。契约把这条记为 Known gap，这两条测试把它钉死。
  it("shows the trust badge alongside the connection badge", () => {
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard device={onlineDevice} displayName="Remote Mac" />
      </I18nProvider>,
    );

    expect(screen.getByTestId("connection-badge")).toBeTruthy();
    expect(screen.getByText("协作者")).toBeTruthy();
  });

  it("keeps the unconfirmed trust hint visible while connected", () => {
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard
          device={{ ...onlineDevice, trustConfirmed: false }}
          displayName="Remote Mac"
        />
      </I18nProvider>,
    );

    // 徽标里是「· 待确认」（前缀点与文案在同一个 span），所以按子串匹配。
    expect(screen.getByText(/待确认/)).toBeTruthy();
  });
});

// 「阻止」是用户明确表态过不要跟这台设备来往的一档。此前整卡点击与发送按钮只判在线，
// 于是一台**在线的已阻止设备**卡片可点、按钮高亮，点下去才由内核拒绝。
describe("DeviceCard blocked device", () => {
  const blockedDevice: Device = {
    ...offlineDevice,
    status: "online",
    trustLevel: "blocked",
  };

  it("disables sending even when the device is online", () => {
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard device={blockedDevice} displayName="Remote Mac" onSend={vi.fn()} />
      </I18nProvider>,
    );

    expect(screen.getByTestId("device-send-action").hasAttribute("disabled")).toBe(true);
  });

  it("does not turn the whole card into a send target", async () => {
    const user = userEvent.setup();
    const onSend = vi.fn();
    render(
      <I18nProvider i18n={i18n}>
        <DeviceCard device={blockedDevice} displayName="Remote Mac" onSend={onSend} />
      </I18nProvider>,
    );

    await user.click(screen.getByTestId("device-card"));
    expect(onSend).not.toHaveBeenCalled();
  });
});
