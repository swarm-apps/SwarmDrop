// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const { confirmTextDelivery, pendingTextDeliveries, refreshInbox } = vi.hoisted(() => ({
  confirmTextDelivery: vi.fn(async () => undefined),
  pendingTextDeliveries: vi.fn(),
  refreshInbox: vi.fn(),
}));

vi.mock("../_lib/node-runtime", () => ({
  getNode: () => ({
    confirm_text_delivery: confirmTextDelivery,
    pending_text_deliveries: pendingTextDeliveries,
  }),
}));

vi.mock("../_lib/store", () => ({
  useWebNode: (selector: (state: unknown) => unknown) =>
    selector({ status: "running", textDeliveryRevision: 0 }),
  webNodeActions: { refreshInbox },
}));

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
vi.mock("@/components/ui/button", () => ({
  Button: ({ children, ...props }: React.ComponentProps<"button">) => <button {...props}>{children}</button>,
}));
vi.mock("@/components/ui/alert-dialog", () => ({
  AlertDialog: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  AlertDialogContent: ({ children, ...props }: React.ComponentProps<"div">) => <div {...props}>{children}</div>,
  AlertDialogDescription: ({ children }: { children: React.ReactNode }) => <p>{children}</p>,
  AlertDialogFooter: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  AlertDialogHeader: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  AlertDialogTitle: ({ children }: { children: React.ReactNode }) => <h2>{children}</h2>,
}));

import { TextDeliveryAttentionHost } from "./text-delivery-attention-host";

const first = {
  deliveryId: "first",
  peerId: "peer-a",
  peerName: "Alice",
  body: "第一条文本",
  createdAt: 1,
};
const second = {
  deliveryId: "second",
  peerId: "peer-b",
  peerName: "Bob",
  body: "第二条文本",
  createdAt: 2,
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  pendingTextDeliveries.mockReset();
});

describe("TextDeliveryAttentionHost", () => {
  it("只展示队首，拒绝后继续处理下一条文本", async () => {
    pendingTextDeliveries.mockResolvedValueOnce([first, second]).mockResolvedValue([second]);
    const user = userEvent.setup();
    render(<TextDeliveryAttentionHost />);

    expect(await screen.findByText("第一条文本")).toBeTruthy();
    expect(screen.queryByText("第二条文本")).toBeNull();
    await user.click(screen.getByRole("button", { name: "拒绝" }));

    await waitFor(() => expect(confirmTextDelivery).toHaveBeenCalledWith("first", false));
    expect(await screen.findByText("第二条文本")).toBeTruthy();
  });

  it("接收时刷新收件箱，而不是把下一条确认混成自动接收", async () => {
    pendingTextDeliveries.mockResolvedValue([first]);
    const user = userEvent.setup();
    render(<TextDeliveryAttentionHost />);

    await screen.findByText("第一条文本");
    await user.click(screen.getByRole("button", { name: "接收" }));

    await waitFor(() => expect(confirmTextDelivery).toHaveBeenCalledWith("first", true));
    expect(refreshInbox).toHaveBeenCalledTimes(1);
  });
});
