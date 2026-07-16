import { $$, browser, expect } from "@wdio/globals";
import {
  byTestId,
  completeOnboardingIfNeeded,
  existsByTestId,
} from "../helpers/ios";

async function openFixture(
  count: 1 | 100 | 1_000 | 10_000,
  scope: "send" | "transfer" | "inbox" = "send",
) {
  if (!(await existsByTestId("file-browser-fixture-screen", 1_000))) {
    await completeOnboardingIfNeeded();
    const fixtureEntry = byTestId("devices-open-file-browser-fixture");
    if (!(await fixtureEntry.isDisplayed())) {
      await browser.execute("mobile: scroll", {
        name: "devices-open-file-browser-fixture",
        direction: "down",
      });
    }
    await fixtureEntry.click();
    await byTestId("file-browser-fixture-screen").waitForDisplayed({
      timeout: 15_000,
    });
  }
  await byTestId(`file-browser-fixture-scope-${scope}`).click();
  await byTestId(`file-browser-fixture-count-${count}`).click();
  await byTestId("file-browser-fixture-ready").waitForDisplayed({
    timeout: 30_000,
  });
  await expect(byTestId("file-browser-fixture-model-pass")).toBeDisplayed();
  await expect(byTestId("file-browser-fixture-send-tree")).toBeDisplayed();
}

describe("SwarmDrop file browser fixtures", () => {
  before(async () => {
    await completeOnboardingIfNeeded();
  });

  it("keeps identities, adapters and directory boundaries stable", async () => {
    await openFixture(100, "send");
    await expect(byTestId("file-browser-fixture-foobar-present")).toBeDisplayed();
    await byTestId("file-browser-fixture-remove-foo").click();
    await expect(byTestId("file-browser-fixture-foobar-present")).toBeDisplayed();
    await expect(byTestId("file-browser-fixture-send-tree")).toBeDisplayed();

    const gridToggle = byTestId("file-browser-fixture-toolbar-view-grid");
    const toggleSize = await gridToggle.getSize();
    expect(toggleSize.width).toBeGreaterThanOrEqual(44);
    expect(toggleSize.height).toBeGreaterThanOrEqual(44);
    await gridToggle.click();
    await expect(byTestId("file-browser-fixture-grid-list")).toBeDisplayed();
    await expect(byTestId("file-browser-fixture-send-grid")).toBeDisplayed();
    await expect(byTestId("file-browser-fixture-transfer-tree")).toBeDisplayed();
    await expect(byTestId("file-browser-fixture-inbox-grid")).toBeDisplayed();
  });

  for (const count of [1, 100, 1_000, 10_000] as const) {
    it(`virtualizes and reaches the end of ${count} files`, async () => {
      await openFixture(count, "send");

      const mountedTreeFiles = await $$
        ('//*[starts-with(@name, "file-browser-fixture-file-")]')
        .getElements();
      const mountedTreeDirectories = await $$
        ('//*[starts-with(@name, "file-browser-fixture-directory-")]')
        .getElements();
      expect(
        mountedTreeFiles.length + mountedTreeDirectories.length,
      ).toBeLessThanOrEqual(Math.min(count + 2, 40));

      await byTestId("file-browser-fixture-toolbar-view-grid").click();
      await expect(byTestId("file-browser-fixture-grid-list")).toBeDisplayed();
      await expect(byTestId("file-browser-fixture-file-0")).toBeDisplayed();

      const mountedRows = await $$
        ('//*[starts-with(@name, "file-browser-fixture-file-")]')
        .getElements();
      expect(mountedRows.length).toBeLessThanOrEqual(Math.min(count, 40));

      const lastId = `file-browser-fixture-file-${count - 1}`;
      if (!(await existsByTestId(lastId, 1_000))) {
        await byTestId("file-browser-fixture-jump-end").click();
      }
      await byTestId(lastId).waitForDisplayed({ timeout: 30_000 });
    });
  }
});
