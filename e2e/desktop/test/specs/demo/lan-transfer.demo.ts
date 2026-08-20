import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  captureDemoFrame,
  pauseForRecording,
  waitForRecorderClose,
  waitForRecorderStart,
  waitForDesktopShell,
} from "./helpers";

async function findFirstDisplayed(
  selector: string,
): Promise<WebdriverIO.Element | null> {
  const elements = await $$(selector);
  for (const element of elements) {
    if (await element.isDisplayed().catch(() => false)) {
      return element;
    }
  }
  return null;
}

async function waitForDisplayedElement(
  selector: string,
  description: string,
  timeout = 60_000,
) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeout) {
    const element = await findFirstDisplayed(selector);
    if (element) return element;
    await browser.pause(500);
  }

  throw new Error(`Timed out waiting for ${description}: ${selector}`);
}

async function ensureDesktopNodeRunning() {
  const offlineStart = await findFirstDisplayed(
    '[data-testid="offline-start-node-action"]',
  );
  if (!offlineStart) return;

  await offlineStart.click();
  const confirm = await waitForDisplayedElement(
    '[data-testid="start-node-confirm-action"]',
    "start node confirm action",
    15_000,
  );
  await confirm.click();
  await waitForDisplayedElement(
    '[data-testid="desktop-home-overview"]',
    "desktop overview after node start",
    60_000,
  );
}

async function findPairedDemoDeviceCard() {
  const expectedName = process.env.SWARMDROP_E2E_DEVICE_NAME ?? "iOS Demo";
  const cards = await $$(
    [
      '[data-testid="device-card"][data-device-paired="true"]',
      '[data-testid="nearby-device-row"][data-device-paired="true"]',
    ].join(", "),
  );
  for (const card of cards) {
    const text = await card.getText().catch(() => "");
    if (text.includes(expectedName)) return card;
  }
  return null;
}

type DemoDeviceSnapshot = {
  matchedPeerIds: string[];
  matchedUnpairedPeerIds: string[];
  devices: Array<{
    peerId: string;
    paired: boolean;
    testId: string | null;
    text: string;
  }>;
};

async function collectDemoDeviceSnapshot(
  expectedName: string,
): Promise<DemoDeviceSnapshot> {
  return await browser.execute((name: string) => {
    const nodes = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="device-card"], [data-testid="nearby-device-row"]',
      ),
    );
    const devices = nodes.map((element) => ({
      peerId: element.dataset.peerId ?? "",
      paired: element.dataset.devicePaired === "true",
      testId: element.dataset.testid ?? null,
      text: (element.textContent ?? "").replace(/\s+/g, " ").trim(),
    }));
    const matchedPeerIds = [
      ...new Set(
        devices
          .filter(
            (device) =>
              device.paired &&
              device.peerId.length > 0 &&
              device.text.includes(name),
          )
          .map((device) => device.peerId),
      ),
    ];
    const matchedUnpairedPeerIds = [
      ...new Set(
        devices
          .filter(
            (device) =>
              !device.paired &&
              device.peerId.length > 0 &&
              device.text.includes(name),
          )
          .map((device) => device.peerId),
      ),
    ];

    return {
      matchedPeerIds,
      matchedUnpairedPeerIds,
      devices: devices
        .filter((device) => device.paired || device.text.includes(name))
        .slice(0, 20),
    };
  }, expectedName);
}

async function removePairedDeviceViaTauri(peerId: string) {
  await browser.tauri.execute(
    ({ core }, id) => core.invoke("remove_paired_device", { peerId: id }),
    peerId,
  );
}

/**
 * 已配对设备列表的落点。
 *
 * 桌面身份存储于 2026-08 从系统 keychain 改为文件后端，同时把密钥与设备列表拆成两个
 * 文件——这里要动的只是后者（`paired-devices.json`，顶层就是设备数组）。
 */
function desktopPairedDevicesFilePath() {
  return (
    process.env.SWARMDROP_DESKTOP_PAIRED_DEVICES_FILE ??
    join(
      homedir(),
      "Library/Application Support/com.yexiyue.swarmdrop/paired-devices.json",
    )
  );
}

function removePeersFromDesktopIdentity(peerIds: string[]) {
  const devicesPath = desktopPairedDevicesFilePath();
  if (!existsSync(devicesPath)) return;

  const devices = JSON.parse(readFileSync(devicesPath, "utf8")) as Array<{
    peerId?: string;
  }>;
  if (!Array.isArray(devices)) return;

  const peerSet = new Set(peerIds);
  const nextDevices = devices.filter(
    (device) => !device.peerId || !peerSet.has(device.peerId),
  );
  if (nextDevices.length === devices.length) return;

  writeFileSync(devicesPath, `${JSON.stringify(nextDevices, null, 2)}\n`);
}

async function removeExistingDemoPairing() {
  const expectedName = process.env.SWARMDROP_E2E_DEVICE_NAME ?? "iOS Demo";
  const cleanupTimeout = Number(
    process.env.SWARMDROP_DEMO_PAIRING_CLEANUP_MS ?? "60000",
  );
  const startedAt = Date.now();
  let snapshot: DemoDeviceSnapshot = {
    matchedPeerIds: [],
    matchedUnpairedPeerIds: [],
    devices: [],
  };

  while (Date.now() - startedAt < cleanupTimeout) {
    snapshot = await collectDemoDeviceSnapshot(expectedName);
    if (
      snapshot.matchedPeerIds.length > 0 ||
      snapshot.matchedUnpairedPeerIds.length > 0
    ) {
      break;
    }
    await browser.pause(500);
  }

  console.log(
    "[recording] desktop demo pairing snapshot",
    JSON.stringify(snapshot),
  );

  if (snapshot.matchedUnpairedPeerIds.length > 0) return;
  if (snapshot.matchedPeerIds.length === 0) {
    throw new Error(
      `Timed out waiting for ${expectedName} to appear before recording`,
    );
  }
  for (const peerId of snapshot.matchedPeerIds) {
    await removePairedDeviceViaTauri(peerId);
  }
  removePeersFromDesktopIdentity(snapshot.matchedPeerIds);

  await browser.execute(() => {
    window.location.reload();
  });
  await browser.pause(1_000);
  await waitForDesktopShell();
  await ensureDesktopNodeRunning();
  await browser.waitUntil(
    async () => (await findPairedDemoDeviceCard()) === null,
    {
      timeout: 15_000,
      timeoutMsg: "Timed out waiting for existing demo pairing to be removed",
    },
  );
}

async function waitForUnpairedLanDevice() {
  return await waitForDisplayedElement(
    [
      '[data-testid="nearby-device-row"][data-device-paired="false"]',
      '[data-testid="device-connect-action"]',
    ].join(", "),
    "an unpaired LAN device",
    90_000,
  );
}

async function waitForPairedSendTarget() {
  return await waitForDisplayedElement(
    [
      '[data-testid="nearby-device-row"][data-device-paired="true"]',
      '[data-testid="device-send-action"]:not([disabled])',
    ].join(", "),
    "paired online send target",
    90_000,
  );
}

/**
 * 把 fixture 投进发送页。
 *
 * **拖放走 Tauri 的窗口级事件，DOM 上没有 `onDrop` 可派发**（见
 * `src/hooks/use-file-drop.ts`）：Tauri v2 的 webview 会把 OS 拖放截走，所以
 * `dispatchEvent(new DragEvent("drop"))` 是彻底的 no-op——这个脚本此前正是那么写的
 * （还伪造了 v2 早已移除的 `File.path`），改成窗口级事件后它整段静默失效。
 *
 * 这里改为 emit 那条系统事件本身。emit 走 `browser.tauri.emitEvent`（插件的一等公民
 * API，经后端真实 `event.emit()` 派发），不要退回裸 `window.__TAURI__` 探测。
 *
 * `position` 的值**无所谓**（hook 不看坐标——那个字段的单位三平台不一致，见
 * `use-file-drop.ts`），但**字段本身不能省**：`@tauri-apps/api` 的 `onDragDropEvent`
 * 包装层会无条件 `new PhysicalPosition(payload.position)`，传 undefined 会在
 * `'Physical' in args[0]` 处抛 TypeError——抛在包装层里，业务 handler 一次都不会跑，
 * 表现又是「投放毫无反应」。
 *
 * 此前这里按 DPR 把 CSS 坐标乘回「物理」像素，正好抵消 hook 里的 `toLogical(DPR)`
 * ——一个恒等 round-trip，无论生产代码对错都会绿，等于没测。
 */
async function dropFixtureIntoSendPage(fixturePath: string) {
  const dropZone = await $('[data-testid="file-drop-zone"]');
  await expect(dropZone).toBeDisplayed({ wait: 15_000 });

  const payload = { paths: [fixturePath], position: { x: 0, y: 0 } };

  // 先 enter 再 drop：这是**录制**脚本，投放区点亮的那一拍要留在片子里。
  // 只发 drop 的话高亮态一帧都不会出现。
  await browser.tauri.emitEvent("tauri://drag-enter", payload);
  await pauseForRecording();
  await browser.tauri.emitEvent("tauri://drag-drop", payload);

  // 投放是异步链路（emit → 后端广播 → listener → store）。在这里等结果落地，
  // 失败原因才留在这一步，而不是 30 秒后从「发送按钮没启用」倒推。
  // 30s 沿用调用点原本的预算：这一步覆盖的东西比那时更多（emit 往返 + 后端广播 +
  // listener → addSources + scanSources 的 I/O + isScanning 清零），录制机负载高时
  // 大文件会超过 15s。改的只是把报错信息指向真正的根因。
  await browser.waitUntil(
    async () => await (await $('[data-testid="send-confirm-action"]')).isEnabled(),
    {
      timeout: 30_000,
      timeoutMsg: `拖放未生效：${basename(fixturePath)} 没有进入发送选择`,
    },
  );
}

describe("SwarmDrop LAN transfer recording", () => {
  it("discovers, pairs, and sends a fixture file to iOS", async () => {
    const fixturePath = resolve(
      process.env.SWARMDROP_TRANSFER_FIXTURE ??
        "fixtures/release-notes.md",
    );
    if (!existsSync(fixturePath)) {
      throw new Error(`Missing transfer fixture: ${fixturePath}`);
    }

    await waitForDesktopShell();
    await ensureDesktopNodeRunning();
    await removeExistingDemoPairing();
    await waitForRecorderStart();
    await captureDemoFrame("lan-transfer-ready");

    const unpairedDevice = await waitForUnpairedLanDevice();
    await pauseForRecording();
    await unpairedDevice.click();

    const pairedTarget = await waitForPairedSendTarget();
    await pauseForRecording();
    await pairedTarget.click();

    await expect($('[data-testid="send-page"]')).toBeDisplayed({
      wait: 30_000,
    });
    await dropFixtureIntoSendPage(fixturePath);
    await pauseForRecording();

    // 「按钮已启用」由 dropFixtureIntoSendPage 等过了（那里的报错信息指向真正的根因）。
    const sendButton = await $('[data-testid="send-confirm-action"]');
    await sendButton.click();

    await expect($('[data-testid="send-progress-view"]')).toBeDisplayed({
      wait: 60_000,
    });
    await pauseForRecording();
    await captureDemoFrame("lan-transfer-started");

    await browser.waitUntil(
      async () => {
        if (await $('[data-testid="send-failure-state"]').isDisplayed()) {
          throw new Error("Desktop transfer entered the failure state");
        }
        return await $('[data-testid="send-success-state"]').isDisplayed();
      },
      {
        timeout: 120_000,
        timeoutMsg: "Timed out waiting for desktop transfer success",
        interval: 500,
      },
    );
    await pauseForRecording();
    await waitForRecorderClose();
  });
});
