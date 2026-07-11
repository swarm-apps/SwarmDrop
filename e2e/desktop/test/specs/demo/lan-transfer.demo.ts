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

function desktopIdentityFilePath() {
  return (
    process.env.SWARMDROP_DESKTOP_IDENTITY_FILE ??
    join(
      homedir(),
      "Library/Application Support/com.yexiyue.swarmdrop/dev-identity.json",
    )
  );
}

function removePeersFromDesktopIdentity(peerIds: string[]) {
  const identityPath = desktopIdentityFilePath();
  if (!existsSync(identityPath)) return;

  const identity = JSON.parse(readFileSync(identityPath, "utf8")) as {
    pairedDevices?: Array<{ peerId?: string }>;
  };
  if (!Array.isArray(identity.pairedDevices)) return;

  const peerSet = new Set(peerIds);
  const nextDevices = identity.pairedDevices.filter(
    (device) => !device.peerId || !peerSet.has(device.peerId),
  );
  if (nextDevices.length === identity.pairedDevices.length) return;

  writeFileSync(
    identityPath,
    `${JSON.stringify({ ...identity, pairedDevices: nextDevices }, null, 2)}\n`,
  );
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

async function dropFixtureIntoSendPage(fixturePath: string) {
  const dropZone = await $('[data-testid="file-drop-zone"]');
  await expect(dropZone).toBeDisplayed({ wait: 15_000 });

  await browser.execute(
    (selector: string, path: string, name: string) => {
      const target = document.querySelector(selector);
      if (!target) throw new Error(`Missing drop target: ${selector}`);

      const file = new File(["SwarmDrop demo fixture\n"], name, {
        type: "text/markdown",
      }) as File & { path?: string };
      Object.defineProperty(file, "path", {
        configurable: true,
        value: path,
      });

      const item = {
        kind: "file",
        type: file.type,
        getAsFile: () => file,
      };
      const dataTransfer = {
        files: [file],
        items: [item],
        types: ["Files"],
      };

      for (const eventName of ["dragover", "drop"]) {
        const event = new DragEvent(eventName, {
          bubbles: true,
          cancelable: true,
        });
        Object.defineProperty(event, "dataTransfer", {
          configurable: true,
          value: dataTransfer,
        });
        target.dispatchEvent(event);
      }
    },
    '[data-testid="file-drop-zone"]',
    fixturePath,
    basename(fixturePath),
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

    const sendButton = await $('[data-testid="send-confirm-action"]');
    await browser.waitUntil(async () => await sendButton.isEnabled(), {
      timeout: 30_000,
      timeoutMsg: "Timed out waiting for send button to become enabled",
    });
    await pauseForRecording();
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
