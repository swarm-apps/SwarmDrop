import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const setFileBrowserView = vi.fn();
const dismissOffer = vi.fn();
const removeOffer = vi.fn();

// `vi.mock` 的工厂被提升到文件顶部，而 `@/lib/bindings` 的那个工厂在对象字面量里**直接**
// 取这两个 spy（不像 store 那个只是闭包引用、渲染时才求值），所以它们必须走 `vi.hoisted`
// ——否则报 "Cannot access 'rejectReceive' before initialization"。
const { acceptReceive, rejectReceive } = vi.hoisted(() => ({
  acceptReceive: vi.fn(async () => undefined),
  rejectReceive: vi.fn(async () => undefined),
}));

function makeOffer(sessionId: string, deviceName: string) {
  return {
    sessionId,
    peerId: `peer-${sessionId}`,
    deviceName,
    files: [
      {
        fileId: 1,
        name: "photo.jpg",
        relativePath: "photos/photo.jpg",
        size: 1024,
        isDirectory: false,
      },
    ],
    totalSize: 1024,
    origin: { type: "human" as const },
    policyAction: null,
    policyReason: "设备接收策略要求手动确认",
  };
}

const offer = makeOffer("offer-1", "V2425A");

/** 每个用例可改的 store 快照。 */
let storeState = {
  pendingOffers: [offer] as ReturnType<typeof makeOffer>[],
  dismissedOfferIds: [] as string[],
  dismissOffer,
  removeOffer,
  restoreOffer: vi.fn(),
  loadProjections: vi.fn(),
};

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => vi.fn(),
}));

vi.mock("@/stores/transfer-store", () => ({
  useTransferStore: (selector: (state: unknown) => unknown) =>
    selector(storeState),
}));

vi.mock("@/lib/bindings", () => ({
  commands: { acceptReceive, rejectReceive },
}));

vi.mock("@/stores/preferences-store", () => ({
  usePreferencesStore: (selector: (state: unknown) => unknown) =>
    selector({
      transfer: { savePath: "C:\\Downloads\\SwarmDrop" },
      fileBrowserViews: { transfer: "tree" },
      setFileBrowserView,
    }),
}));

vi.mock("@/lib/file-picker", () => ({
  pickFolder: vi.fn(async () => null),
  getDefaultSavePath: vi.fn(async () => "C:\\Downloads\\SwarmDrop"),
}));

import { TransferOfferDialog } from "./transfer-offer-dialog";

afterEach(() => {
  cleanup();
  setFileBrowserView.mockClear();
  dismissOffer.mockClear();
  removeOffer.mockClear();
  rejectReceive.mockClear();
  storeState = {
    ...storeState,
    pendingOffers: [offer],
    dismissedOfferIds: [],
  };
});

function renderDialog() {
  return render(
    <I18nProvider i18n={i18n}>
      <TransferOfferDialog />
    </I18nProvider>,
  );
}

describe("TransferOfferDialog", () => {
  it("uses the wide compact layout and supports grid view", async () => {
    const user = userEvent.setup();
    renderDialog();

    const dialog = screen.getByTestId("transfer-offer-dialog");
    expect(dialog.className).toContain("sm:max-w-2xl");
    expect(screen.getByRole("button", { name: "树形视图" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "网格视图" }));
    expect(setFileBrowserView).toHaveBeenCalledWith("transfer", "grid");
  });

  // DESIGN.md 的 Incoming Request Contract：文件 offer 的关闭 **≠** 拒绝。
  // 这条曾经反着写（关闭直接 reject），且关闭按钮与点外部都被堵死——唯一出口 Esc
  // 一按就作废对方整次传输。这三条测试合起来钉住修正后的语义。
  it("关闭弹窗只是暂时收起，不拒绝对方的传输", async () => {
    const user = userEvent.setup();
    renderDialog();

    await user.keyboard("{Escape}");

    expect(dismissOffer).toHaveBeenCalledWith("offer-1");
    expect(rejectReceive).not.toHaveBeenCalled();
    expect(removeOffer).not.toHaveBeenCalled();
  });

  it("关闭出口本身要在场：有可见的关闭按钮", () => {
    renderDialog();
    expect(screen.getByRole("button", { name: "Close" })).toBeTruthy();
  });

  it("被收起的 offer 不再挡住队列里后面那条", () => {
    storeState = {
      ...storeState,
      pendingOffers: [offer, makeOffer("offer-2", "MacBook")],
      dismissedOfferIds: ["offer-1"],
    };
    renderDialog();

    // 此前的判据是「队首且队首没被收起」，收起一条会把后面全部堵死。
    expect(screen.getByText(/MacBook/)).toBeTruthy();
  });

  it("多于一条在等时要说还剩几条", () => {
    storeState = {
      ...storeState,
      pendingOffers: [offer, makeOffer("offer-2", "MacBook")],
    };
    renderDialog();

    expect(screen.getByText(/还有 1 条在等待/)).toBeTruthy();
  });
});
