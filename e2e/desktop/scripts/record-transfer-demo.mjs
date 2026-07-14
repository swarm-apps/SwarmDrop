#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomUUID } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import net from "node:net";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const desktopDir = resolve(scriptDir, "..");
const repoRoot = resolve(desktopDir, "../..");
const mobileRoot = resolve(repoRoot, "../SwarmDrop-RN");
const desktopTransferSpec = resolve(
  desktopDir,
  "test/specs/demo/lan-transfer.demo.ts",
);
const mobileTransferSpec = resolve(
  mobileRoot,
  "e2e/webdriver/test/specs/accept-transfer.e2e.ts",
);

function parseArgs(argv) {
  // pnpm 将 package script 的参数分隔符 `--` 原样传入；本脚本没有子命令边界，直接忽略。
  const args = argv.filter((arg) => arg !== "--");
  const options = {
    noRecord: false,
    skipBuild: false,
    noMobileServer: false,
    desktopOnly: false,
    mobileOnly: false,
    keepMobileState: false,
  };

  for (const arg of args) {
    if (arg === "--no-record") {
      options.noRecord = true;
    } else if (arg === "--skip-build") {
      options.skipBuild = true;
    } else if (arg === "--no-mobile-server") {
      options.noMobileServer = true;
    } else if (arg === "--desktop-only") {
      options.desktopOnly = true;
    } else if (arg === "--mobile-only") {
      options.mobileOnly = true;
    } else if (arg === "--keep-mobile-state") {
      options.keepMobileState = true;
    } else {
      throw new Error(`Unknown option: ${arg}`);
    }
  }

  return options;
}

function expandHome(value) {
  if (!value.startsWith("~/")) return value;
  return join(process.env.HOME ?? "", value.slice(2));
}

function defaultObsConfigPath() {
  return join(
    process.env.HOME ?? "",
    "Library/Application Support/obs-studio/plugin_config/obs-websocket/config.json",
  );
}

function readObsPassword() {
  if (process.env.OBS_WEBSOCKET_PASSWORD) {
    return process.env.OBS_WEBSOCKET_PASSWORD;
  }

  const configPath = expandHome(
    process.env.OBS_WEBSOCKET_CONFIG ?? defaultObsConfigPath(),
  );
  if (!existsSync(configPath)) return "";

  const config = JSON.parse(readFileSync(configPath, "utf8"));
  return config.server_password ?? "";
}

function resolveSimulatorUdid() {
  if (process.env.SWARMDROP_IOS_UDID) return process.env.SWARMDROP_IOS_UDID;

  const deviceName = process.env.SWARMDROP_E2E_DEVICE_NAME ?? "iPhone 17 Pro";
  const result = spawnSync("xcrun", ["simctl", "list", "devices"], {
    encoding: "utf8",
  });
  if (result.status !== 0) return null;

  const line = result.stdout
    .split("\n")
    .find((entry) => entry.includes(`${deviceName} (`));
  return line?.match(/\(([0-9A-F-]{36})\)/i)?.[1] ?? null;
}

async function startSimulatorRecording(path, udid) {
  const child = spawn(
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
    {
      cwd: repoRoot,
      env: { ...process.env },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  const recorder = {
    child,
    result: null,
    done: new Promise((resolveDone) => {
      child.on("exit", (code, signal) => {
        recorder.result = { code, signal };
        resolveDone(recorder.result);
      });
    }),
  };

  await new Promise((resolveStart, rejectStart) => {
    let output = "";
    const timeout = setTimeout(() => {
      rejectStart(new Error(`Simulator recording did not start: ${output}`));
    }, 5_000);
    const onData = (chunk) => {
      output += chunk.toString();
      if (output.includes("Recording started")) {
        clearTimeout(timeout);
        resolveStart();
      }
    };
    recorder.child.stdout?.on("data", onData);
    recorder.child.stderr?.on("data", onData);
    recorder.child.once("error", (error) => {
      clearTimeout(timeout);
      rejectStart(error);
    });
  });

  return recorder;
}

async function stopSimulatorRecording(recorder) {
  if (!recorder) return;
  recorder.child.kill("SIGINT");
  await recorder.done;
}

function authDigest(value) {
  return createHash("sha256").update(value).digest("base64");
}

function makeObsAuth(password, salt, challenge) {
  const secret = authDigest(password + salt);
  return authDigest(secret + challenge);
}

class ObsClient {
  constructor(url, password) {
    this.url = url;
    this.password = password;
    this.ws = null;
    this.pending = new Map();
  }

  async connect() {
    this.ws = new WebSocket(this.url);
    this.ws.addEventListener("message", (event) => this.handleMessage(event.data));
    this.ws.addEventListener("close", () => this.rejectAll("OBS WebSocket closed"));
    this.ws.addEventListener("error", () => this.rejectAll("OBS WebSocket error"));

    await new Promise((resolveOpen, rejectOpen) => {
      const timer = setTimeout(
        () => rejectOpen(new Error(`Timed out connecting to ${this.url}`)),
        3_000,
      );
      this.ws.addEventListener(
        "open",
        () => {
          clearTimeout(timer);
          resolveOpen();
        },
        { once: true },
      );
      this.ws.addEventListener(
        "error",
        () => {
          clearTimeout(timer);
          rejectOpen(new Error(`Unable to connect to ${this.url}`));
        },
        { once: true },
      );
    });

    await new Promise((resolveIdentified, rejectIdentified) => {
      const timer = setTimeout(
        () => rejectIdentified(new Error("Timed out waiting for OBS Identify")),
        5_000,
      );
      this.resolveIdentified = (data) => {
        clearTimeout(timer);
        resolveIdentified(data);
      };
      this.rejectIdentified = (error) => {
        clearTimeout(timer);
        rejectIdentified(error);
      };
    });
  }

  handleMessage(raw) {
    const message = JSON.parse(String(raw));
    if (message.op === 0) {
      const auth = message.d?.authentication;
      this.send(1, {
        rpcVersion: 1,
        ...(auth
          ? {
              authentication: makeObsAuth(
                this.password,
                auth.salt,
                auth.challenge,
              ),
            }
          : {}),
      });
      return;
    }

    if (message.op === 2) {
      this.resolveIdentified?.(message.d ?? {});
      return;
    }

    if (message.op === 7) {
      const data = message.d ?? {};
      const pending = this.pending.get(data.requestId);
      if (!pending) return;

      this.pending.delete(data.requestId);
      if (data.requestStatus?.result) {
        pending.resolve(data.responseData ?? {});
      } else {
        const comment = data.requestStatus?.comment ?? "request failed";
        pending.reject(new Error(`${data.requestType}: ${comment}`));
      }
    }
  }

  send(op, data) {
    this.ws.send(JSON.stringify({ op, d: data }));
  }

  request(requestType, requestData = undefined) {
    const requestId = randomUUID();
    return new Promise((resolveRequest, rejectRequest) => {
      this.pending.set(requestId, {
        resolve: resolveRequest,
        reject: rejectRequest,
      });
      this.send(6, {
        requestType,
        requestId,
        ...(requestData ? { requestData } : {}),
      });
    });
  }

  rejectAll(reason) {
    const error = new Error(reason);
    this.rejectIdentified?.(error);
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }

  close() {
    this.ws?.close();
  }
}

function openObs() {
  if (process.platform !== "darwin") return;
  spawnSync("open", ["-a", "OBS"], { stdio: "ignore" });
}

function hideObsWindow() {
  if (process.platform !== "darwin") return;
  if (process.env.SWARMDROP_OBS_HIDE_WINDOW === "0") return;

  spawnSync(
    "osascript",
    ["-e", 'tell application "System Events" to set visible of process "OBS" to false'],
    { stdio: "ignore" },
  );
}

function activateSimulatorWindow() {
  if (process.platform !== "darwin") return;
  if (process.env.SWARMDROP_OBS_ACTIVATE_SIMULATOR === "0") return;

  spawnSync("osascript", ["-e", 'tell application "Simulator" to activate'], {
    stdio: "ignore",
  });
}

async function connectObs() {
  const url = process.env.OBS_WEBSOCKET_URL ?? "ws://127.0.0.1:4455";
  openObs();

  let lastError = null;
  for (let i = 0; i < 15; i += 1) {
    try {
      const client = new ObsClient(url, readObsPassword());
      await client.connect();
      return client;
    } catch (error) {
      lastError = error;
      await delay(1_000);
    }
  }

  throw lastError ?? new Error(`Unable to connect to OBS at ${url}`);
}

async function waitForObsRecordReady(obs, timeout = 15_000) {
  const startedAt = Date.now();
  let lastStatus = null;

  while (Date.now() - startedAt < timeout) {
    lastStatus = await obs.request("GetRecordStatus");
    if (!lastStatus.outputActive && !lastStatus.outputPaused) {
      return lastStatus;
    }
    await delay(500);
  }

  throw new Error(
    `OBS is not ready to record: ${JSON.stringify(lastStatus ?? {})}`,
  );
}

async function startObsRecording(obs, timeout = 15_000) {
  const startedAt = Date.now();
  let lastError = null;

  while (Date.now() - startedAt < timeout) {
    await waitForObsRecordReady(obs, 5_000);
    try {
      await obs.request("StartRecord");
      return;
    } catch (error) {
      lastError = error;
      await delay(750);
    }
  }

  const status = await obs.request("GetRecordStatus").catch(() => null);
  const message = lastError instanceof Error ? lastError.message : String(lastError);
  const suffix = status ? ` status=${JSON.stringify(status)}` : "";
  throw new Error(`${message}${suffix}`);
}

function spawnCommand(command, args, options = {}) {
  const state = {
    child: null,
    result: null,
    done: null,
  };
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    stdio: "inherit",
  });

  state.child = child;
  state.done = new Promise((resolveDone) => {
    child.on("exit", (code, signal) => {
      state.result = { code, signal };
      resolveDone(state.result);
    });
  });
  return state;
}

function runCommand(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    stdio: "inherit",
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed`);
  }
}

async function isPortOpen(port, host = "127.0.0.1") {
  return await new Promise((resolveCheck) => {
    const socket = net.createConnection({ host, port });
    socket.once("connect", () => {
      socket.destroy();
      resolveCheck(true);
    });
    socket.once("error", () => resolveCheck(false));
  });
}

async function waitForPort(port, host = "127.0.0.1", timeout = 30_000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeout) {
    if (await isPortOpen(port, host)) return;
    await delay(500);
  }

  throw new Error(`Timed out waiting for ${host}:${port}`);
}

function writeSignalFiles(files, content) {
  for (const file of files) {
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, content);
  }
}

async function waitForSignalFiles(files, runs, label, timeout = 180_000) {
  const pending = new Set(files);
  const startedAt = Date.now();
  while (pending.size > 0) {
    for (const file of [...pending]) {
      if (existsSync(file)) pending.delete(file);
    }
    if (pending.size === 0) return;

    const exited = runs.find((run) => run.result);
    if (exited) {
      throw new Error(
        `Demo runner exited before ${label}: code=${exited.result.code} signal=${exited.result.signal}`,
      );
    }

    if (Date.now() - startedAt > timeout) {
      throw new Error(
        `Timed out waiting for demo ${label}: ${[...pending].join(", ")}`,
      );
    }
    await delay(250);
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (!existsSync(mobileRoot)) {
    throw new Error(`Mobile repo not found: ${mobileRoot}`);
  }

  let mobileServer = null;
  let mobileRecorder = null;
  let obs = null;
  let recordingStarted = false;
  let recordingStopped = false;
  let closeSignalsWritten = false;
  const runs = [];
  const closeSignalFiles = [];

  try {
    if (!options.skipBuild && !options.mobileOnly) {
      runCommand("pnpm", ["tauri", "build", "--debug", "--no-bundle"], {
        cwd: repoRoot,
        env: { VITE_WDIO_TAURI_PLUGIN: "1" },
      });
    }

    if (!options.noMobileServer && !options.desktopOnly) {
      if (await isPortOpen(8081)) {
        console.log("Metro dev server already listening on 127.0.0.1:8081");
      } else {
        mobileServer = spawnCommand("pnpm", ["start"], {
          cwd: mobileRoot,
        }).child;
      }
      await waitForPort(8081);
    }

    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const signalDir = resolve(desktopDir, "build/desktop-recordings/signals", stamp);
    mkdirSync(signalDir, { recursive: true });
    const desktopReadyFile = join(signalDir, "desktop-ready");
    const desktopGoFile = join(signalDir, "desktop-go");
    const desktopDoneFile = join(signalDir, "desktop-done");
    const desktopCloseFile = join(signalDir, "desktop-close");
    const mobileReadyFile = join(signalDir, "mobile-ready");
    const mobileGoFile = join(signalDir, "mobile-go");
    const mobileDoneFile = join(signalDir, "mobile-done");
    const mobileCloseFile = join(signalDir, "mobile-close");
    const mobileRecordingPath = resolve(
      desktopDir,
      "build/desktop-recordings/raw",
      `ios-transfer-${stamp}.mp4`,
    );
    const expectedReadyFiles = [];
    const expectedDoneFiles = [];

    const simulatorUdid =
      !options.noRecord && !options.desktopOnly
        ? resolveSimulatorUdid()
        : null;
    if (simulatorUdid) {
      mobileRecorder = await startSimulatorRecording(
        mobileRecordingPath,
        simulatorUdid,
      );
    }

    if (!options.mobileOnly) {
      expectedReadyFiles.push(desktopReadyFile);
      expectedDoneFiles.push(desktopDoneFile);
      closeSignalFiles.push(desktopCloseFile);
      runs.push(
        spawnCommand(
          "pnpm",
          [
            "--dir",
            desktopDir,
            "exec",
            "wdio",
            "run",
            "./wdio.conf.ts",
            "--spec",
            desktopTransferSpec,
          ],
          {
            cwd: repoRoot,
            env: {
              SWARMDROP_DEMO_RECORDING: "1",
              SWARMDROP_DEMO_READY_FILE: desktopReadyFile,
              SWARMDROP_DEMO_GO_FILE: desktopGoFile,
              SWARMDROP_DEMO_DONE_FILE: desktopDoneFile,
              SWARMDROP_DEMO_CLOSE_FILE: desktopCloseFile,
              SWARMDROP_DEMO_STEP_DELAY_MS:
                process.env.SWARMDROP_DEMO_STEP_DELAY_MS ?? "1000",
            },
          },
        ),
      );
    }

    if (!options.desktopOnly) {
      expectedReadyFiles.push(mobileReadyFile);
      expectedDoneFiles.push(mobileDoneFile);
      closeSignalFiles.push(mobileCloseFile);
      runs.push(
        spawnCommand(
          "pnpm",
          [
            "exec",
            "wdio",
            "run",
            "./e2e/webdriver/wdio.ios.conf.ts",
            "--spec",
            mobileTransferSpec,
          ],
          {
            cwd: mobileRoot,
            env: {
              SWARMDROP_IOS_NO_RESET:
                process.env.SWARMDROP_IOS_NO_RESET ??
                (options.keepMobileState ? "1" : "0"),
              SWARMDROP_E2E_RECORDING: "1",
              SWARMDROP_E2E_DEVICE_NAME:
                process.env.SWARMDROP_E2E_DEVICE_NAME ?? "iOS Demo",
              SWARMDROP_MOBILE_READY_FILE: mobileReadyFile,
              SWARMDROP_MOBILE_GO_FILE: mobileGoFile,
              SWARMDROP_MOBILE_DONE_FILE: mobileDoneFile,
              SWARMDROP_MOBILE_CLOSE_FILE: mobileCloseFile,
              ...(options.noRecord
                ? {}
                : {
                    SWARMDROP_MOBILE_RECORDING_PATH: mobileRecordingPath,
                    ...(mobileRecorder
                      ? { SWARMDROP_MOBILE_RECORDING_EXTERNAL: "1" }
                      : {}),
                  }),
              SWARMDROP_DEMO_STEP_DELAY_MS:
                process.env.SWARMDROP_DEMO_STEP_DELAY_MS ?? "1000",
              SWARMDROP_E2E_STEP_DELAY_MS:
                process.env.SWARMDROP_E2E_STEP_DELAY_MS ??
                process.env.SWARMDROP_DEMO_STEP_DELAY_MS ??
                "1000",
            },
          },
        ),
      );
    }

    await waitForSignalFiles(expectedReadyFiles, runs, "readiness");

    if (!options.noRecord) {
      obs = await connectObs();
      hideObsWindow();
      activateSimulatorWindow();
      await startObsRecording(obs);
      recordingStarted = true;
      await delay(Number(process.env.SWARMDROP_DEMO_RECORDING_DELAY_MS ?? "1000"));
    }

    if (!options.mobileOnly) writeFileSync(desktopGoFile, "go\n");
    if (!options.desktopOnly) writeFileSync(mobileGoFile, "go\n");

    await waitForSignalFiles(expectedDoneFiles, runs, "flow completion", 240_000);

    if (obs && recordingStarted) {
      const result = await obs.request("StopRecord");
      recordingStopped = true;
      if (result?.outputPath) {
        console.log(`OBS recording: ${result.outputPath}`);
      }
    }

    await stopSimulatorRecording(mobileRecorder);
    mobileRecorder = null;

    writeSignalFiles(closeSignalFiles, "close\n");
    closeSignalsWritten = true;

    const results = await Promise.all(runs.map((run) => run.done));
    if (!options.noRecord && existsSync(mobileRecordingPath)) {
      console.log(`Mobile recording: ${mobileRecordingPath}`);
    }
    const failed = results.find((result) => result.code !== 0);
    if (failed) {
      process.exitCode = failed.code ?? 1;
    }
  } finally {
    if (obs && recordingStarted && !recordingStopped) {
      const result = await obs.request("StopRecord").catch((error) => {
        console.error(error);
        return null;
      });
      if (result?.outputPath) {
        console.log(`OBS recording: ${result.outputPath}`);
      }
    }
    if (!closeSignalsWritten) {
      writeSignalFiles(closeSignalFiles, "close\n");
    }
    obs?.close();
    await stopSimulatorRecording(mobileRecorder).catch((error) => {
      console.error(error);
    });
    mobileRecorder = null;
    for (const run of runs) {
      if (!run.result) run.child?.kill("SIGINT");
    }
    mobileServer?.kill("SIGINT");
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
