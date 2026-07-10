import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { dirname } from "node:path";
import { $, browser } from "@wdio/globals";

const defaultTimeout = 15_000;

export function demoStepDelay(fallback = 1_000) {
  const value = Number(
    process.env.SWARMDROP_E2E_STEP_DELAY_MS ??
      process.env.SWARMDROP_DEMO_STEP_DELAY_MS ??
      fallback,
  );
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

export async function pauseForRecording(ms = demoStepDelay()) {
  await browser.pause(ms);
}

function mobileRecordingPath() {
  return process.env.SWARMDROP_MOBILE_RECORDING_PATH;
}

let simulatorRecorder: ChildProcess | null = null;

function simulatorUdid() {
  const capabilities = browser.capabilities as Record<string, unknown>;
  const explicitUdid =
    process.env.SWARMDROP_IOS_UDID ??
    (capabilities["appium:udid"] as string | undefined) ??
    (capabilities.udid as string | undefined);
  if (explicitUdid) return explicitUdid;

  const deviceName =
    process.env.SWARMDROP_IOS_DEVICE_NAME ??
    (capabilities["appium:deviceName"] as string | undefined);
  if (!deviceName) return undefined;

  const result = spawnSync("xcrun", ["simctl", "list", "devices"], {
    encoding: "utf8",
  });
  const line = result.stdout
    .split("\n")
    .find((entry) => entry.includes(`${deviceName} (`));
  return line?.match(/\(([0-9A-F-]{36})\)/i)?.[1];
}

function isSimulator(udid: string | undefined) {
  if (!udid) return false;
  const result = spawnSync("xcrun", ["simctl", "list", "devices"], {
    encoding: "utf8",
  });
  return result.status === 0 && result.stdout.includes(`(${udid})`);
}

async function startSimulatorRecording(path: string, udid: string) {
  const recorder = spawn(
    "xcrun",
    [
      "simctl",
      "io",
      udid,
      "recordVideo",
      `--display=${process.env.SWARMDROP_IOS_DISPLAY ?? "1"}`,
      "--codec=h264",
      "--force",
      path,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  simulatorRecorder = recorder;

  await new Promise<void>((resolve, reject) => {
    let output = "";
    const timeout = setTimeout(() => {
      if (output.includes("Recording started")) {
        resolve();
      } else {
        recorder.kill("SIGINT");
        reject(new Error(`Simulator recording did not start: ${output}`));
      }
    }, 5_000);

    const onData = (chunk: Buffer) => {
      output += chunk.toString();
      if (output.includes("Recording started")) {
        clearTimeout(timeout);
        resolve();
      }
    };
    recorder.stdout?.on("data", onData);
    recorder.stderr?.on("data", onData);
    recorder.once("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    recorder.once("exit", (code) => {
      if (code !== null && !output.includes("Recording started")) {
        clearTimeout(timeout);
        reject(new Error(`Simulator recording exited with code ${code}: ${output}`));
      }
    });
  });
}

async function stopSimulatorRecording(path: string) {
  const recorder = simulatorRecorder;
  simulatorRecorder = null;
  if (!recorder) return;

  await new Promise<void>((resolve) => {
    recorder.once("close", () => resolve());
    recorder.kill("SIGINT");
  });

  if (!existsSync(path)) {
    throw new Error(`Simulator did not write the mobile screen recording: ${path}`);
  }
}

export async function startMobileRecording() {
  const path = mobileRecordingPath();
  if (!path) return false;
  if (process.env.SWARMDROP_MOBILE_RECORDING_EXTERNAL === "1") return true;

  mkdirSync(dirname(path), { recursive: true });
  rmSync(path, { force: true });
  const udid = simulatorUdid();
  if (isSimulator(udid)) {
    await startSimulatorRecording(path, udid!);
  } else {
    await browser.startRecordingScreen({
      forceRestart: true,
      videoType: "libx264",
      videoFps: 30,
      pixelFormat: "yuv420p",
    });
  }
  return true;
}

export async function stopMobileRecording() {
  const path = mobileRecordingPath();
  if (!path) return false;
  if (process.env.SWARMDROP_MOBILE_RECORDING_EXTERNAL === "1") return true;

  if (simulatorRecorder) {
    await stopSimulatorRecording(path);
    return true;
  }

  const recording = await browser.stopRecordingScreen();
  if (!recording) {
    throw new Error("Appium returned an empty mobile screen recording");
  }

  writeFileSync(path, Buffer.from(recording, "base64"));
  return true;
}

export function byTestId(testId: string) {
  return $(`~${testId}`);
}

export async function existsByTestId(testId: string, timeout = 1_000) {
  const element = byTestId(testId);
  try {
    await element.waitForExist({ timeout });
    return true;
  } catch {
    return false;
  }
}

export async function tapIfExists(testId: string, timeout = 1_000) {
  if (!(await existsByTestId(testId, timeout))) return false;
  await byTestId(testId).click();
  return true;
}

export async function tapAccessibilityLabelIfExists(
  label: string,
  timeout = 1_000,
) {
  const element = $(`~${label}`);
  try {
    await element.waitForExist({ timeout });
    await element.click();
    return true;
  } catch {
    return false;
  }
}

export async function waitForAnyTestId(
  testIds: string[],
  timeout = defaultTimeout,
) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeout) {
    for (const testId of testIds) {
      if (await existsByTestId(testId, 300)) {
        return testId;
      }
    }
    await browser.pause(250);
  }

  throw new Error(`Timed out waiting for any testID: ${testIds.join(", ")}`);
}

export async function dismissExpoWarningToast() {
  const warning = await $('//*[contains(@name, "Open debugger")]');
  if (!(await warning.isExisting())) return;

  const size = await browser.getWindowSize();
  await browser
    .execute("mobile: tap", {
      x: Math.round(size.width * 0.93),
      y: Math.round(size.height * 0.9),
    })
    .catch(() => undefined);
}

export async function hideKeyboardIfNeeded() {
  await browser.hideKeyboard().catch(async () => {
    await browser.keys(["Enter"]).catch(() => undefined);
  });
}

export async function completeOnboardingIfNeeded() {
  await browser.pause(1_000);
  await dismissExpoWarningToast();

  const initialScreen = await waitForAnyTestId(
    [
      "onboarding-start-button",
      "onboarding-device-name-input",
      "onboarding-enter-button",
      "devices-screen",
      "devices-header",
      "file-browser-fixture-screen",
    ],
    30_000,
  );
  const alreadyInApp =
    initialScreen === "devices-screen" ||
    initialScreen === "devices-header" ||
    initialScreen === "file-browser-fixture-screen";

  if (initialScreen === "onboarding-start-button") {
    await byTestId("onboarding-start-button").click();
  }

  if (await existsByTestId("onboarding-device-name-input", 5_000)) {
    const input = byTestId("onboarding-device-name-input");
    await input.click();
    await input.clearValue();
    await input.setValue(process.env.SWARMDROP_E2E_DEVICE_NAME ?? "iOS Demo");
    await hideKeyboardIfNeeded();
    await dismissExpoWarningToast();
    await byTestId("onboarding-device-name-continue-button").click();
  }

  if (
    !alreadyInApp &&
    (await existsByTestId("onboarding-enter-button", 30_000))
  ) {
    await dismissExpoWarningToast();
    await byTestId("onboarding-enter-button").click();
  }

  if (!alreadyInApp) {
    await tapIfExists("onboarding-enter-button", 1_000);
  }

  return await waitForAnyTestId(
    [
      "devices-screen",
      "devices-header",
      "inbox-header",
      "settings-header",
      "file-browser-fixture-screen",
    ],
    45_000,
  );
}

export async function waitForRecorderStart() {
  const readyFile = process.env.SWARMDROP_MOBILE_READY_FILE;
  const goFile = process.env.SWARMDROP_MOBILE_GO_FILE;
  if (!readyFile || !goFile) return;

  mkdirSync(dirname(readyFile), { recursive: true });
  writeFileSync(readyFile, "ready\n");

  const startedAt = Date.now();
  while (!existsSync(goFile)) {
    if (Date.now() - startedAt > 60_000) {
      throw new Error("Timed out waiting for mobile demo recorder");
    }
    await browser.pause(100);
  }
}

export async function waitForRecorderClose() {
  const doneFile = process.env.SWARMDROP_MOBILE_DONE_FILE;
  const closeFile = process.env.SWARMDROP_MOBILE_CLOSE_FILE;
  if (!doneFile || !closeFile) return;

  mkdirSync(dirname(doneFile), { recursive: true });
  writeFileSync(doneFile, "done\n");

  const startedAt = Date.now();
  while (!existsSync(closeFile)) {
    if (Date.now() - startedAt > 120_000) {
      throw new Error("Timed out waiting for mobile demo recorder close");
    }
    await browser.pause(100);
  }
}
