import { $, expect } from "@wdio/globals";
import {
  captureDemoFrame,
  pauseForRecording,
  waitForDesktopHome,
} from "./helpers";

describe("SwarmDrop desktop home demo", () => {
  it("records the desktop device center overview", async function () {
    this.timeout(180_000);

    await waitForDesktopHome();

    await expect($('[data-testid="desktop-home-overview"]')).toBeDisplayed();
    await expect($('[data-testid="paired-devices-section"]')).toBeDisplayed();
    await expect($('[data-testid="add-device-section"]')).toBeDisplayed();

    await pauseForRecording(300);
    await captureDemoFrame("desktop-home-overview");
    await pauseForRecording(500);
  });
});
