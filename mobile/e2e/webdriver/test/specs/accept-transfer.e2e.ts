import { $, browser, expect } from "@wdio/globals";
import {
  byTestId,
  completeOnboardingIfNeeded,
  dismissExpoWarningToast,
  existsByTestId,
  pauseForRecording,
  startMobileRecording,
  stopMobileRecording,
  tapAccessibilityLabelIfExists,
  tapIfExists,
  waitForAnyTestId,
  waitForRecorderClose,
  waitForRecorderStart,
} from "../helpers/ios";

async function openDevicesScreen() {
  const shell = await completeOnboardingIfNeeded();
  if (shell === "devices-screen" || shell === "devices-header") return;

  if (!(await tapAccessibilityLabelIfExists("设备", 2_000))) {
    await $("//*[contains(@name, \"设备\")]").click();
  }

  await waitForAnyTestId(["devices-screen", "devices-header"], 15_000);
}

async function ensureNodeRunning() {
  if (await existsByTestId("devices-start-node-button", 1_000)) {
    await byTestId("devices-start-node-button").click();
  } else if (await existsByTestId("devices-retry-node-button", 1_000)) {
    await byTestId("devices-retry-node-button").click();
  }

  await waitForAnyTestId(
    ["devices-add-device-button", "devices-local-code"],
    60_000,
  );
}

describe("SwarmDrop iOS transfer receiver", () => {
  it("accepts desktop pairing and incoming transfer", async () => {
    let recordingStarted = false;
    try {
      await openDevicesScreen();
      await ensureNodeRunning();
      recordingStarted = await startMobileRecording();
      await waitForRecorderStart();

      let acceptedPairing = false;
      let acceptedTransfer = false;
      const startedAt = Date.now();

      while (Date.now() - startedAt < 120_000) {
        await dismissExpoWarningToast();

        if (!acceptedPairing) {
          acceptedPairing =
            (await tapIfExists("pairing-request-accept-button", 500)) ||
            (await tapAccessibilityLabelIfExists("接受", 500));
          if (acceptedPairing) {
            await pauseForRecording();
            continue;
          }
        }

        if (await existsByTestId("transfer-offer-dialog", 500)) {
          await pauseForRecording();
          await byTestId("transfer-offer-accept-button").click();
          acceptedTransfer = true;
          await pauseForRecording();
          break;
        }

        await browser.pause(500);
      }

      expect(acceptedPairing).toBe(true);
      expect(acceptedTransfer).toBe(true);

      await waitForAnyTestId(
        ["transfer-success-state", "transfer-failure-state"],
        120_000,
      );
      if (await existsByTestId("transfer-failure-state", 500)) {
        throw new Error("Mobile transfer entered the failure state");
      }
      await waitForRecorderClose();
    } finally {
      if (recordingStarted) await stopMobileRecording();
    }
  });
});
