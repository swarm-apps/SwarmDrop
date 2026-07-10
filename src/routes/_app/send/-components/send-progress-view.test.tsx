import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const projection = {
  sessionId: "session-1",
  peerName: "V2425A",
  phase: "terminal",
  savePath: null,
};

vi.mock("@/stores/transfer-store", () => ({
  useTransferStore: (selector: (state: unknown) => unknown) =>
    selector({ projections: { "session-1": projection } }),
  useSessionProgress: () => null,
}));

vi.mock("@/components/transfer/session-panel", () => ({
  SessionSummaryHeader: () => <div data-testid="session-summary" />,
  SessionProgressBlock: () => <div data-testid="session-progress" />,
  SessionFileSection: () => <div data-testid="session-files" />,
  SessionActions: () => <div data-testid="session-actions" />,
}));

vi.mock("@/lib/transfer-projection", () => ({
  isProjectionActive: () => false,
  isProjectionCompleted: () => true,
}));

import { SendProgressView } from "./send-progress-view";

afterEach(cleanup);

describe("SendProgressView layout", () => {
  it("keeps actions outside the middle scroll region and reserves file height", () => {
    render(
      <I18nProvider i18n={i18n}>
        <SendProgressView
          sessionId="session-1"
          onBack={vi.fn()}
          onSessionChange={vi.fn()}
        />
      </I18nProvider>,
    );

    const scrollRegion = screen.getByTestId("send-progress-scroll-region");
    expect(scrollRegion.className).toContain("overflow-auto");
    expect(scrollRegion.contains(screen.getByTestId("session-summary"))).toBe(true);
    expect(scrollRegion.contains(screen.getByTestId("session-files"))).toBe(true);
    expect(scrollRegion.contains(screen.getByTestId("session-actions"))).toBe(false);

    const filePanel = screen.getByTestId("session-files").closest("section");
    expect(filePanel?.className).toContain("h-[360px]");
    expect(filePanel?.className).toContain("lg:h-[440px]");
  });
});
