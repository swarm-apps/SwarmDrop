import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const {
  confirmTextDelivery,
  emitAttention,
  navigate,
  pendingTextDeliveries,
  listen,
} = vi.hoisted(() => {
  let attentionListener:
    | ((event: { payload: { kind: "confirmation_required" | "received"; peerName: string } }) => void)
    | undefined;
  return {
    confirmTextDelivery: vi.fn(async () => undefined),
    emitAttention: (payload: { kind: "confirmation_required" | "received"; peerName: string }) =>
      attentionListener?.({ payload }),
    navigate: vi.fn(),
    pendingTextDeliveries: vi.fn(),
    listen: vi.fn(async (
      _eventName: string,
      listener: typeof attentionListener,
    ) => {
      attentionListener = listener;
      return () => undefined;
    }),
  };
});

vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }));
vi.mock("@/lib/bindings", () => ({
  commands: {
    confirmTextDelivery,
    pendingTextDeliveries,
    listInboxItems: vi.fn(async () => []),
  },
}));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));

import { TextDeliveryAttentionHost } from "./text-delivery-attention-host";

const first = {
  deliveryId: "first",
  peerId: "peer-a",
  peerName: "Alice",
  body: "first body",
  createdAt: 1,
};
const second = {
  deliveryId: "second",
  peerId: "peer-b",
  peerName: "Bob",
  body: "second body",
  createdAt: 2,
};

afterEach(() => {
  cleanup();
  confirmTextDelivery.mockClear();
  pendingTextDeliveries.mockReset();
  listen.mockClear();
  navigate.mockClear();
  vi.restoreAllMocks();
});

describe("TextDeliveryAttentionHost", () => {
  it("只显示队首，并在拒绝后继续下一条待确认文本", async () => {
    pendingTextDeliveries.mockResolvedValueOnce([first, second]).mockResolvedValue([second]);
    const user = userEvent.setup();
    render(
      <I18nProvider i18n={i18n}>
        <TextDeliveryAttentionHost />
      </I18nProvider>,
    );

    expect(await screen.findByText("first body")).toBeTruthy();
    expect(screen.queryByText("second body")).toBeNull();
    await user.click(screen.getByRole("button", { name: "拒绝" }));

    await waitFor(() =>
      expect(confirmTextDelivery).toHaveBeenCalledWith("first", false),
    );
    expect(await screen.findByText("second body")).toBeTruthy();
  });

  it("确认只提交队首的投递标识", async () => {
    pendingTextDeliveries.mockResolvedValue([first]);
    const user = userEvent.setup();
    render(
      <I18nProvider i18n={i18n}>
        <TextDeliveryAttentionHost />
      </I18nProvider>,
    );

    await screen.findByText("first body");
    await user.click(screen.getByRole("button", { name: "接收" }));
    await waitFor(() =>
      expect(confirmTextDelivery).toHaveBeenCalledWith("first", true),
    );
  });

  it("后台自动接收只给出非阻塞反馈，并在重新获焦时定位收件箱", async () => {
    pendingTextDeliveries.mockResolvedValue([]);
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    render(
      <I18nProvider i18n={i18n}>
        <TextDeliveryAttentionHost />
      </I18nProvider>,
    );

    await waitFor(() => expect(listen).toHaveBeenCalledTimes(1));
    emitAttention({ kind: "received", peerName: "Alice" });
    window.dispatchEvent(new Event("focus"));

    expect(navigate).toHaveBeenCalledWith({ to: "/inbox" });
    expect(screen.queryByTestId("text-delivery-confirmation")).toBeNull();
  });
});
