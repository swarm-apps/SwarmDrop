import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { $, browser, expect } from "@wdio/globals";

const screenshotDir = resolve(process.cwd(), "build/wdio/screenshots");
let recordingWindowPinned = false;
let recorderStarted = false;

export function demoStepDelay(fallback = 1_000) {
  const value = Number(process.env.SWARMDROP_DEMO_STEP_DELAY_MS ?? fallback);
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

async function pinRecordingWindow() {
  if (recordingWindowPinned || process.env.SWARMDROP_DEMO_RECORDING !== "1") {
    return;
  }

  recordingWindowPinned = true;
  try {
    await browser.tauri.switchWindow("main");
  } catch (error) {
    console.warn("[recording] Failed to pin Tauri main window:", error);
  }
}

export async function waitForDesktopShell() {
  await pinRecordingWindow();
  await expect(browser).toHaveTitle("SwarmDrop", { wait: 30_000 });
  await expect($("#root")).toBeExisting();
  await expect($('[data-testid="desktop-devices-page"]')).toBeDisplayed({
    wait: 30_000,
  });
}

export async function waitForDesktopHome() {
  await waitForDesktopShell();
  await waitForRecorderStart();
}

export async function waitForRecorderStart() {
  if (recorderStarted) return;

  const readyFile = process.env.SWARMDROP_DEMO_READY_FILE;
  const goFile = process.env.SWARMDROP_DEMO_GO_FILE;
  if (!readyFile || !goFile) return;

  recorderStarted = true;
  mkdirSync(dirname(readyFile), { recursive: true });
  writeFileSync(readyFile, "ready\n");

  const startedAt = Date.now();
  while (!existsSync(goFile)) {
    if (Date.now() - startedAt > 120_000) {
      throw new Error("Timed out waiting for desktop demo recorder");
    }
    await browser.pause(100);
  }
}

export async function waitForRecorderClose() {
  const doneFile = process.env.SWARMDROP_DEMO_DONE_FILE;
  const closeFile = process.env.SWARMDROP_DEMO_CLOSE_FILE;
  if (!doneFile || !closeFile) return;

  mkdirSync(dirname(doneFile), { recursive: true });
  writeFileSync(doneFile, "done\n");

  const startedAt = Date.now();
  while (!existsSync(closeFile)) {
    if (Date.now() - startedAt > 120_000) {
      throw new Error("Timed out waiting for desktop demo recorder close");
    }
    await browser.pause(100);
  }
}

export async function captureDemoFrame(name: string) {
  mkdirSync(screenshotDir, { recursive: true });
  await browser.saveScreenshot(resolve(screenshotDir, `${name}.png`));
}

export async function pauseForRecording(ms = demoStepDelay()) {
  await browser.pause(ms);
}
