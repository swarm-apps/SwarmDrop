import { $, $$, expect } from "@wdio/globals";
import {
  captureDemoFrame,
  pauseForRecording,
  waitForDesktopHome,
} from "./helpers";

describe("SwarmDrop desktop inbox demo", () => {
  it("records the inbox surface", async function () {
    this.timeout(180_000);

    await waitForDesktopHome();
    await pauseForRecording(250);

    await $('[data-testid="topbar-inbox-link"]').click();

    await expect($('[data-testid="inbox-page"]')).toBeDisplayed({
      wait: 10_000,
    });
    await expect($('[data-testid="inbox-rail"]')).toBeDisplayed();

    const items = await $$('[data-testid="inbox-item"]');
    const itemCount = await items.length;
    if (itemCount > 0) {
      await items[0].click();
      await expect($('[data-testid="inbox-reader"]')).toBeDisplayed();
    } else {
      await expect(
        $('[data-testid="inbox-empty-state"], [data-testid="inbox-reader-placeholder"]'),
      ).toBeDisplayed();
    }

    await pauseForRecording(300);
    await captureDemoFrame("inbox");
    await pauseForRecording(500);
  });
});
