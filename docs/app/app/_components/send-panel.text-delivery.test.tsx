// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const {
  deleteTextOutboxRecord,
  listTextOutbox,
  readClipboard,
  retryTextDelivery,
  sendTextDelivery,
} = vi.hoisted(() => ({
  deleteTextOutboxRecord: vi.fn(async () => undefined),
  listTextOutbox: vi.fn(async () => []),
  readClipboard: vi.fn(async () => ""),
  retryTextDelivery: vi.fn(async () => undefined),
  sendTextDelivery: vi.fn(async () => undefined),
}));

const device = {
  peerId: "peer-a",
  name: "Alice",
  hostname: "alice",
  os: "Windows",
  platform: "desktop",
  arch: "x64",
  capabilities: [],
  status: "online",
  isPaired: true,
} as never;

vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.ComponentProps<"a">) => <a {...props}>{children}</a>,
}));
vi.mock("next/navigation", () => ({
  useSearchParams: () => new URLSearchParams("peerId=peer-a"),
}));
vi.mock("@/components/ui/button", () => ({
  Button: ({ children, ...props }: React.ComponentProps<"button">) => <button {...props}>{children}</button>,
}));
vi.mock("@/components/ui/select", () => ({
  Select: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children: React.ReactNode }) => <button type="button">{children}</button>,
  SelectValue: () => null,
}));
vi.mock("@swarmdrop/file-browser", () => ({ FileBrowser: () => <div /> }));
vi.mock("../_lib/node-runtime", () => ({
  getNode: () => ({
    delete_text_outbox_record: deleteTextOutboxRecord,
    list_text_outbox: listTextOutbox,
    retry_text_delivery: retryTextDelivery,
    send_text_delivery: sendTextDelivery,
  }),
}));
vi.mock("../_lib/clipboard", () => ({ clipboard: { readText: readClipboard } }));
vi.mock("../_lib/store", () => ({
  useWebNode: (selector: (state: unknown) => unknown) => selector({
    pairedDevices: [device],
    progress: {},
    projections: {},
    rejections: {},
    status: "running",
  }),
  webNodeActions: { clearPrepare: vi.fn() },
}));
vi.mock("../_lib/preferences-store", () => ({
  preferencesActions: { setFileBrowserView: vi.fn() },
  usePreferences: (selector: (state: unknown) => unknown) => selector({
    deviceOrganization: { aliases: {}, groups: {} },
    fileBrowserViews: { send: "tree" },
  }),
}));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

import { SendPanel } from "./send-panel";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  listTextOutbox.mockResolvedValue([]);
  readClipboard.mockResolvedValue("");
});

async function openTextEditor() {
  const user = userEvent.setup();
  render(<SendPanel />);
  // `tab` 而不是 `button`：内容模式切换器是 tablist/tab（三端统一的语义），
  // 且它现在住在目标设备那一行里，不再独占一行。
  await user.click(screen.getByRole("tab", { name: "文本" }));
  return user;
}

describe("SendPanel text delivery", () => {
  it("切换文本模式后提供粘贴、清空、UTF-8 限制与发送操作", async () => {
    await openTextEditor();

    expect(screen.getByLabelText("文本内容")).toBeTruthy();
    expect(screen.getByRole("button", { name: "粘贴" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "清空" })).toBeTruthy();
    expect(screen.getByText("0 KiB / 64 KiB")).toBeTruthy();
    expect(screen.getByRole("button", { name: "发送" })).toHaveProperty("disabled", true);
  });

  it("按 UTF-8 字节数拦截超限文本", async () => {
    await openTextEditor();

    fireEvent.change(screen.getByLabelText("文本内容"), {
      target: { value: "🚀".repeat(16_385) },
    });

    expect(screen.getByText("文本超过 64 KiB，请缩短后发送。")).toBeTruthy();
    expect(screen.getByRole("button", { name: "发送" })).toHaveProperty("disabled", true);
  });

  it("粘贴后发送，并为可重试记录提供重试操作", async () => {
    listTextOutbox.mockResolvedValue([
      {
        deliveryId: "retry-1",
        peerId: "peer-a",
        peerName: "Alice",
        body: "稍后重试的文本",
        status: "retryable",
        attemptCount: 1,
        createdAt: 1,
        updatedAt: 1,
      },
    ] as never);
    readClipboard.mockResolvedValue("来自剪贴板的文本");
    const user = await openTextEditor();

    expect(await screen.findByText("稍后重试的文本")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(retryTextDelivery).toHaveBeenCalledWith("retry-1"));

    await user.click(screen.getByRole("button", { name: "粘贴" }));
    await waitFor(() =>
      expect((screen.getByLabelText("文本内容") as HTMLTextAreaElement).value).toBe(
        "来自剪贴板的文本",
      ),
    );
    await user.click(screen.getByRole("button", { name: "发送" }));
    await waitFor(() =>
      expect(sendTextDelivery).toHaveBeenCalledWith("peer-a", "来自剪贴板的文本"),
    );
  });
});
