import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const {
  deleteTextOutboxRecord,
  listTextOutbox,
  readText,
  retryTextDelivery,
  sendTextDelivery,
} = vi.hoisted(() => ({
  deleteTextOutboxRecord: vi.fn(async () => undefined),
  listTextOutbox: vi.fn(async () => []),
  readText: vi.fn(async () => ""),
  retryTextDelivery: vi.fn(async () => undefined),
  sendTextDelivery: vi.fn(async () => undefined),
}));

vi.mock("@/lib/bindings", () => ({
  commands: {
    deleteTextOutboxRecord,
    listTextOutbox,
    retryTextDelivery,
    sendTextDelivery,
  },
}));

vi.mock("@/stores/transfer-store", () => ({
  useActivePrepareProgress: () => null,
  useTransferStore: () => vi.fn(),
}));

vi.mock("@/stores/network-store", () => ({ useNetworkStore: () => ({ devices: [] }) }));
vi.mock("@/stores/secret-store", () => ({ useSecretStore: () => ({ pairedDevices: [] }) }));
vi.mock("@/stores/preferences-store", () => ({
  usePreferencesStore: (selector: (state: unknown) => unknown) => selector({
    fileBrowserViews: { send: "tree" },
    setFileBrowserView: vi.fn(),
    deviceOrganization: { aliases: {}, groups: {} },
  }),
}));

vi.mock("@/lib/clipboard", () => ({ readText }));
vi.mock("@/components/pairing/device-icon", () => ({
  getDeviceIcon: () => () => <span />,
}));
vi.mock("@swarmdrop/file-browser", () => ({ FileBrowser: () => <div /> }));
vi.mock("./-use-file-selection", () => ({ useFileSelection: vi.fn() }));
vi.mock("./-components/file-drop-zone", () => ({ FileDropZone: () => <div /> }));
vi.mock("./-components/prepare-progress-bar", () => ({ PrepareProgressBar: () => <div /> }));
vi.mock("./-components/send-progress-view", () => ({ SendProgressView: () => <div /> }));
// 这几个替身都要**透传 props**：`Tabs` / `TabsContent` 经 `asChild` 把 tabpanel 的
// aria 关联与显隐状态落在 `TaskPageShell` / `GlassPanel` 上，吞掉 props 的替身会让这层
// 关系在测试里凭空消失，测出来的东西和真实渲染不是一回事。
vi.mock("@/components/layout/task-surface", () => ({
  CommandDock: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  GlassPanel: ({ children, ...props }: React.ComponentProps<"section">) => <section {...props}>{children}</section>,
  TaskButton: ({ children, ...props }: React.ComponentProps<"button">) => <button {...props}>{children}</button>,
  TaskContent: ({ children, footer }: { children: React.ReactNode; footer: React.ReactNode }) => <div>{children}{footer}</div>,
  TaskPageShell: ({ children, ...props }: React.ComponentProps<"main">) => <main {...props}>{children}</main>,
  // `trailing` 同理——内容模式切换器就住在这个插槽里（见 -components/content-mode-tabs）。
  // 之前这个 mock 是 `() => <div />`，等于把被测组件传进来的东西整块丢掉。
  TaskToolbar: ({ trailing }: { trailing?: React.ReactNode }) => <div>{trailing}</div>,
}));

import { DesktopSendView } from "./index.lazy";

const device = {
  peerId: "peer-a",
  name: "Alice",
  hostname: "alice",
  os: "Windows",
  platform: "desktop",
  arch: "x64",
  capabilities: [],
  status: "online",
  connection: null,
  connectionDetails: null,
  lanUpgradeFailed: false,
  latency: null,
  isPaired: true,
  trustLevel: "collaborator",
  receivePolicy: null,
  trustConfirmed: true,
} as never;

const fileSelection = {
  hasFiles: false,
  totalCount: 0,
  totalSize: 0,
  removeTarget: vi.fn(),
} as never;

function renderSendView() {
  return render(
    <I18nProvider i18n={i18n}>
      <DesktopSendView
        device={device}
        displayName="Alice"
        identityHint="Windows"
        groupNames={[]}
        fileSelection={fileSelection}
        sending={false}
        prepareProgress={null}
        onSourcesSelected={vi.fn()}
        onSend={vi.fn()}
        onBack={vi.fn()}
      />
    </I18nProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  listTextOutbox.mockResolvedValue([]);
  readText.mockResolvedValue("");
});

describe("DesktopSendView text delivery", () => {
  it("切换文本模式后提供粘贴、清空、统一 KiB 限制和发送操作", async () => {
    const user = userEvent.setup();
    renderSendView();

    await user.click(screen.getByRole("tab", { name: "文本" }));

    expect(screen.getByLabelText("要发送的文本")).toBeTruthy();
    expect(screen.getByRole("button", { name: "从剪贴板粘贴" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "清空" })).toBeTruthy();
    expect(screen.getByText("支持 UTF-8 文本，最长 64 KiB")).toBeTruthy();
    expect(screen.getByRole("button", { name: "发送文本" })).toBeTruthy();
  });

  it("以 UTF-8 字节而非字符数拦截超限文本", async () => {
    const user = userEvent.setup();
    renderSendView();
    await user.click(screen.getByRole("tab", { name: "文本" }));

    fireEvent.change(screen.getByLabelText("要发送的文本"), {
      target: { value: "😀".repeat(16_385) },
    });

    expect(screen.getByText("文本超过 64 KiB，请缩短后发送。")).toBeTruthy();
    expect(screen.getByRole("button", { name: "发送文本" })).toHaveProperty(
      "disabled",
      true,
    );
  });

  it("从剪贴板粘贴文本后可直接发送", async () => {
    readText.mockResolvedValue("来自剪贴板的文本");
    const user = userEvent.setup();
    renderSendView();
    await user.click(screen.getByRole("tab", { name: "文本" }));

    await user.click(screen.getByRole("button", { name: "从剪贴板粘贴" }));
    await waitFor(() =>
      expect((screen.getByLabelText("要发送的文本") as HTMLTextAreaElement).value).toBe(
        "来自剪贴板的文本",
      ),
    );
    await user.click(screen.getByRole("button", { name: "发送文本" }));
    await waitFor(() =>
      expect(sendTextDelivery).toHaveBeenCalledWith("peer-a", "Alice", "来自剪贴板的文本"),
    );
  });

  it("发送文本并为可重试记录提供重试入口", async () => {
    listTextOutbox.mockResolvedValue([
      {
        deliveryId: "retry-1",
        peerId: "peer-a",
        peerName: "Alice",
        body: "稍后重试的文本",
        status: "retryable",
        failure: "timed_out",
        attemptCount: 1,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    ] as never);
    const user = userEvent.setup();
    renderSendView();
    await user.click(screen.getByRole("tab", { name: "文本" }));

    expect(await screen.findByText("稍后重试的文本")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(retryTextDelivery).toHaveBeenCalledWith("retry-1"));

    await user.clear(screen.getByLabelText("要发送的文本"));
    await user.type(screen.getByLabelText("要发送的文本"), "你好");
    await user.click(screen.getByRole("button", { name: "发送文本" }));
    await waitFor(() =>
      expect(sendTextDelivery).toHaveBeenCalledWith("peer-a", "Alice", "你好"),
    );
  });
});
