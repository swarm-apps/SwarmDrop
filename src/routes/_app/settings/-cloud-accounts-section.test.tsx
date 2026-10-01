import { I18nProvider } from "@lingui/react";
import { i18n } from "@lingui/core";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  connect: vi.fn(), reconnect: vi.fn(), cancel: vi.fn(), disconnect: vi.fn(), list: vi.fn(), open: vi.fn(), update: null as null | ((event: { payload: unknown }) => void),
}));
vi.mock("@/lib/bindings", () => ({ commands: { listCloudAccounts: mocks.list, connectCloudAccount: mocks.connect, reconnectCloudAccount: mocks.reconnect, cancelCloudAccountConnect: mocks.cancel, disconnectCloudAccount: mocks.disconnect }, events: { cloudAccountUpdated: { listen: vi.fn(async (callback) => { mocks.update = callback; return vi.fn(); }) } } }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.open }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn() } }));
import { CloudAccountsSection } from "./-cloud-accounts-section";
beforeEach(() => { vi.clearAllMocks(); mocks.list.mockResolvedValue([]); mocks.connect.mockResolvedValue({ id: "session", authorizationUrl: "https://accounts.google.com/authorize" }); mocks.open.mockResolvedValue(undefined); });
afterEach(cleanup);
function mount() { return render(<I18nProvider i18n={i18n}><CloudAccountsSection /></I18nProvider>); }

it("未填写完整凭证时不能打开浏览器", async () => {
  mount(); await userEvent.click(screen.getByRole("button", { name: "连接 Google Drive" }));
  expect(mocks.connect).not.toHaveBeenCalled(); expect(mocks.open).not.toHaveBeenCalled();
});
it("开始授权后清空 secret，并通过事件呈现完成状态", async () => {
  mount(); const user = userEvent.setup();
  await user.type(screen.getByLabelText("Client ID"), "client"); await user.type(screen.getByLabelText("Client Secret"), "secret");
  await user.click(screen.getByRole("button", { name: "连接 Google Drive" }));
  await waitFor(() => expect(mocks.open).toHaveBeenCalledWith("https://accounts.google.com/authorize"));
  expect((screen.getByLabelText("Client Secret") as HTMLInputElement).value).toBe("");
  expect(screen.getByText("请在系统浏览器完成授权。")).toBeTruthy();
  mocks.list.mockResolvedValue([{ id: "account", provider: "googleDrive", label: "我的云盘", status: "connected" }]);
  mocks.update?.({ payload: { sessionId: "session", account: { id: "account" }, error: null } });
  await screen.findByText("我的云盘"); expect(screen.queryByText("请在系统浏览器完成授权。")).toBeNull();
});
it("失效账户直接使用已保存配置重新连接", async () => {
  mocks.list.mockResolvedValue([{ id: "account", provider: "googleDrive", label: "我的云盘", status: "reconnectRequired" }]);
  mocks.reconnect.mockResolvedValue({ id: "retry-session", authorizationUrl: "https://accounts.google.com/retry" });
  mount(); await userEvent.click(await screen.findByRole("button", { name: "重新连接" }));
  expect(mocks.reconnect).toHaveBeenCalledWith("account"); expect(mocks.connect).not.toHaveBeenCalled();
});
