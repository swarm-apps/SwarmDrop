import { browser, expect } from "@wdio/globals";
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
    throw new Error("无法打开设备页");
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

describe("SwarmDrop iOS text delivery receiver", () => {
  it("accepts desktop pairing and an incoming text", async () => {
    let recordingStarted = false;
    try {
      await openDevicesScreen();
      await ensureNodeRunning();
      recordingStarted = await startMobileRecording();
      await waitForRecorderStart();

      let acceptedPairing = false;
      const startedAt = Date.now();

      // 外部桌面脚本在收到配对完成后发送一段文本。循环既容忍配对顺序，
      // 也确保不会把其他宿主级确认框误当成文本投递确认。
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

        if (await existsByTestId("text-delivery-confirmation", 500)) break;
        await browser.pause(500);
      }

      expect(acceptedPairing).toBe(true);
      await byTestId("text-delivery-confirmation").waitForExist({
        timeout: 15_000,
      });
      await pauseForRecording();
      await byTestId("text-delivery-accept-button").click();

      await byTestId("text-delivery-confirmation").waitForExist({
        timeout: 15_000,
        reverse: true,
      });
      await waitForRecorderClose();
    } finally {
      if (recordingStarted) await stopMobileRecording();
    }
  });
});
