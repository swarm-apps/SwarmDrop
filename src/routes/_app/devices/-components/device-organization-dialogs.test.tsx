import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Device } from "@/lib/bindings";
import type { DeviceOrganization } from "@/lib/device-organization";
import {
  DeviceGroupsDialog,
  DeviceOrganizationDialog,
} from "./device-organization-dialogs";

const organization: DeviceOrganization = {
  aliases: {},
  groups: [
    { id: "g1", name: "工作", sortOrder: 0 },
    { id: "g2", name: "家庭", sortOrder: 1 },
  ],
  groupDeviceIds: { g1: ["p1", "p2"], g2: ["p3"] },
};

function renderWithI18n(ui: React.ReactElement) {
  return render(<I18nProvider i18n={i18n}>{ui}</I18nProvider>);
}

function groupsActions() {
  return {
    createGroup: vi.fn(() => "new-id"),
    renameGroup: vi.fn(),
    deleteGroup: vi.fn(),
    reorderGroups: vi.fn(),
  };
}

afterEach(cleanup);

describe("DeviceGroupsDialog", () => {
  it("renders each group as a draggable, keyboard-reachable row with member count", () => {
    renderWithI18n(
      <DeviceGroupsDialog
        open
        onOpenChange={vi.fn()}
        organization={organization}
        actions={groupsActions()}
      />,
    );

    // 每组名可编辑
    expect(screen.getByDisplayValue("工作")).toBeTruthy();
    expect(screen.getByDisplayValue("家庭")).toBeTruthy();
    // 拖拽手柄带可达 label（键盘可拿起 → 取代旧的上下箭头）
    expect(screen.getAllByLabelText("拖动排序")).toHaveLength(2);
    // 成员数量以「N 台设备」暴露给读屏
    expect(screen.getByTitle("2 台设备")).toBeTruthy();
    expect(screen.getByTitle("1 台设备")).toBeTruthy();
  });

  it("shows an empty state and hides the drag list when there are no groups", () => {
    renderWithI18n(
      <DeviceGroupsDialog
        open
        onOpenChange={vi.fn()}
        organization={{ aliases: {}, groups: [], groupDeviceIds: {} }}
        actions={groupsActions()}
      />,
    );

    expect(screen.getByText("还没有分组")).toBeTruthy();
    expect(screen.queryByLabelText("拖动排序")).toBeNull();
  });

  it("gates the create button on non-empty input and forwards the new name", async () => {
    const user = userEvent.setup();
    const actions = groupsActions();
    renderWithI18n(
      <DeviceGroupsDialog
        open
        onOpenChange={vi.fn()}
        organization={organization}
        actions={actions}
      />,
    );

    const createBtn = screen.getByRole("button", { name: "新建" });
    expect(createBtn.hasAttribute("disabled")).toBe(true);

    await user.type(screen.getByPlaceholderText("新分组名称"), "实验室");
    expect(createBtn.hasAttribute("disabled")).toBe(false);

    await user.click(createBtn);
    expect(actions.createGroup).toHaveBeenCalledWith("实验室");
  });

  it("confirms deletion by group name before removing it", async () => {
    const user = userEvent.setup();
    const actions = groupsActions();
    renderWithI18n(
      <DeviceGroupsDialog
        open
        onOpenChange={vi.fn()}
        organization={organization}
        actions={actions}
      />,
    );

    await user.click(screen.getByLabelText("删除分组「工作」"));

    const alert = screen.getByRole("alertdialog");
    expect(within(alert).getByText("删除分组「工作」")).toBeTruthy();

    await user.click(within(alert).getByRole("button", { name: "删除" }));
    expect(actions.deleteGroup).toHaveBeenCalledWith("g1");
  });

  it("renames a group on blur", async () => {
    const user = userEvent.setup();
    const actions = groupsActions();
    renderWithI18n(
      <DeviceGroupsDialog
        open
        onOpenChange={vi.fn()}
        organization={organization}
        actions={actions}
      />,
    );

    const input = screen.getByDisplayValue("工作");
    await user.clear(input);
    await user.type(input, "办公室");
    await user.tab();

    expect(actions.renameGroup).toHaveBeenCalledWith("g1", "办公室");
  });
});

describe("DeviceOrganizationDialog", () => {
  const device: Device = {
    peerId: "p1",
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

  function orgActions() {
    return {
      setAlias: vi.fn(),
      setGroups: vi.fn(),
      createGroup: vi.fn(() => "new-id"),
      renameGroup: vi.fn(),
      deleteGroup: vi.fn(),
      reorderGroups: vi.fn(),
    };
  }

  it("toggles group membership as pills and saves alias + groups together", async () => {
    const user = userEvent.setup();
    const actions = orgActions();
    renderWithI18n(
      <DeviceOrganizationDialog
        open
        onOpenChange={vi.fn()}
        device={device}
        organization={organization}
        actions={actions}
      />,
    );

    // p1 预置在「工作」组 → 该 pill 初始为选中态
    const workPill = screen.getByRole("button", { name: "工作", pressed: true });
    expect(workPill).toBeTruthy();

    // 取消「工作」、勾选「家庭」
    await user.click(workPill);
    await user.click(screen.getByRole("button", { name: "家庭" }));

    await user.click(screen.getByRole("button", { name: "保存" }));

    expect(actions.setAlias).toHaveBeenCalledWith("p1", "");
    expect(actions.setGroups).toHaveBeenCalledWith("p1", ["g2"]);
  });
});
