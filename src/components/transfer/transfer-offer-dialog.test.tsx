import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const setFileBrowserView = vi.fn();
const offer = {
  sessionId: "offer-1",
  peerId: "peer-1",
  deviceName: "V2425A",
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

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => vi.fn(),
}));

vi.mock("@/stores/transfer-store", () => ({
  useTransferStore: (selector: (state: unknown) => unknown) =>
    selector({
      pendingOffers: [offer],
      shiftOffer: vi.fn(),
      loadProjections: vi.fn(),
    }),
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
});

describe("TransferOfferDialog", () => {
  it("uses the wide compact layout and supports grid view", async () => {
    const user = userEvent.setup();
    render(
      <I18nProvider i18n={i18n}>
        <TransferOfferDialog />
      </I18nProvider>,
    );

    const dialog = screen.getByTestId("transfer-offer-dialog");
    expect(dialog.className).toContain("sm:max-w-2xl");
    expect(screen.getByRole("button", { name: "树形视图" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "网格视图" }));
    expect(setFileBrowserView).toHaveBeenCalledWith("transfer", "grid");
  });
});
