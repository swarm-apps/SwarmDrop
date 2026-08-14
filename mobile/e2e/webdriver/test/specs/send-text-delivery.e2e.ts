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

describe("SwarmDrop iOS text delivery sender", () => {
  it("pairs with desktop and sends a text from the device flow", async () => {
    let recordingStarted = false;
    try {
      await openDevicesScreen();
      await ensureNodeRunning();
      recordingStarted = await startMobileRecording();
      await waitForRecorderStart();

      let acceptedPairing = false;
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
        if (await existsByTestId("device-card-0", 500)) break;
        await browser.pause(500);
      }

      expect(acceptedPairing).toBe(true);
      await byTestId("device-card-0").click();
      await byTestId("device-detail-send-button").waitForExist({
        timeout: 15_000,
      });
      await byTestId("device-detail-send-button").click();
      await byTestId("send-content-mode-text").waitForExist({
        timeout: 15_000,
      });
      await byTestId("send-content-mode-text").click();
      const editor = byTestId("send-text-editor");
      await editor.setValue("Mobile text delivery end-to-end check");
      expect(await editor.getValue()).toBe("Mobile text delivery end-to-end check");
      await byTestId("send-text-action").click();

      // 外部桌面脚本接受该文本后解除录制屏障；屏障不存在时这是无操作，
      // 使常规真机回归与演示录制共用同一条交互路径。
      await waitForRecorderClose();
    } finally {
      if (recordingStarted) await stopMobileRecording();
    }
  });
});
