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
