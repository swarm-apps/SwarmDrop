#!/usr/bin/env node
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomUUID } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const desktopDir = resolve(scriptDir, "..");
const repoRoot = resolve(desktopDir, "../..");

const demos = {
  "desktop-home": "./test/specs/demo/desktop-home.demo.ts",
  "send-file": "./test/specs/demo/send-file.demo.ts",
  inbox: "./test/specs/demo/inbox.demo.ts",
};

const buildDir = join(desktopDir, "build");
const rawDir = join(buildDir, "desktop-recordings", "raw");
const manifestDir = join(buildDir, "desktop-recordings", "manifests");
const signalDir = join(buildDir, "desktop-recordings", "signals");

function parseArgs(argv) {
  const options = {
    demo: "desktop-home",
    spec: null,
    skipBuild: false,
    noRecord: false,
    openObs: true,
    demoExplicit: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--skip-build") {
      options.skipBuild = true;
    } else if (arg === "--no-record") {
      options.noRecord = true;
    } else if (arg === "--no-open-obs") {
      options.openObs = false;
    } else if (arg === "--spec") {
      options.spec = argv[i + 1] ?? null;
      i += 1;
    } else if (arg.startsWith("--spec=")) {
      options.spec = arg.slice("--spec=".length);
    } else if (arg.startsWith("-")) {
      throw new Error(`Unknown option: ${arg}`);
    } else {
      options.demo = arg;
      options.demoExplicit = true;
    }
  }

  return options;
}

function resolveSpec(options) {
  if (options.spec) return options.spec;
  const spec = demos[options.demo];
  if (!spec) {
    throw new Error(
      `Unknown demo "${options.demo}". Available demos: ${Object.keys(demos).join(", ")}`,
    );
  }
  return spec;
}

function timestamp() {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function demoNameFromSpec(spec) {
  return basename(spec).replace(/\.(demo|e2e)\.ts$/, "").replace(/\.ts$/, "");
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
    this.identified = null;
  }

  async connect() {
    this.ws = new WebSocket(this.url);
    this.ws.addEventListener("message", (event) => this.handleMessage(event.data));
    this.ws.addEventListener("close", () => this.rejectAll("OBS WebSocket closed"));
    this.ws.addEventListener("error", () => this.rejectAll("OBS WebSocket error"));

    await new Promise((resolveOpen, rejectOpen) => {
      const timer = setTimeout(() => {
        rejectOpen(new Error(`Timed out connecting to ${this.url}`));
      }, 3_000);

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

    this.identified = new Promise((resolveIdentified, rejectIdentified) => {
      const timer = setTimeout(() => {
        rejectIdentified(new Error("Timed out waiting for OBS Identify response"));
      }, 5_000);
      this.resolveIdentified = (data) => {
        clearTimeout(timer);
        resolveIdentified(data);
      };
      this.rejectIdentified = (error) => {
        clearTimeout(timer);
        rejectIdentified(error);
      };
    });

    return this.identified;
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
    const payload = {
      requestType,
      requestId,
      ...(requestData ? { requestData } : {}),
    };

    return new Promise((resolveRequest, rejectRequest) => {
      this.pending.set(requestId, {
        resolve: resolveRequest,
        reject: rejectRequest,
      });
      this.send(6, payload);
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

async function connectObs({ openIfNeeded }) {
  const url = process.env.OBS_WEBSOCKET_URL ?? "ws://127.0.0.1:4455";
  const password = readObsPassword();
  let launched = false;
  let lastError = null;

  for (let attempt = 0; attempt < 40; attempt += 1) {
    try {
      const client = new ObsClient(url, password);
      const identified = await client.connect();
      return { client, identified, url };
    } catch (error) {
      lastError = error;
      if (!launched && openIfNeeded) {
        openObs();
        launched = true;
      }
      await delay(500);
    }
  }

  throw lastError ?? new Error(`Unable to connect to OBS at ${url}`);
}

function runCommand(command, args, cwd, extraEnv = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      cwd,
      stdio: "inherit",
      env: { ...process.env, ...extraEnv },
    });

    child.on("error", rejectRun);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        resolveRun();
        return;
      }
      rejectRun(
        new Error(
          `${command} ${args.join(" ")} failed with ${signal ?? `exit code ${code}`}`,
        ),
      );
    });
  });
}

function spawnCommand(command, args, cwd, extraEnv = {}) {
  const child = spawn(command, args, {
    cwd,
    stdio: "inherit",
    env: { ...process.env, ...extraEnv },
  });

  const done = new Promise((resolveRun, rejectRun) => {
    child.on("error", rejectRun);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        resolveRun();
        return;
      }
      rejectRun(
        new Error(
          `${command} ${args.join(" ")} failed with ${signal ?? `exit code ${code}`}`,
        ),
      );
    });
  });

  return { child, done };
}

async function waitForFile(filePath, timeoutMs) {
  const startedAt = Date.now();
  while (!existsSync(filePath)) {
    if (Date.now() - startedAt > timeoutMs) {
      throw new Error(`Timed out waiting for ${filePath}`);
    }
    await delay(100);
  }
}

function removeFile(filePath) {
  rmSync(filePath, { force: true });
}

async function waitForStableFile(filePath, timeoutMs = 10_000) {
  const startedAt = Date.now();
  let lastSize = -1;
  let lastChangedAt = Date.now();

  while (Date.now() - startedAt <= timeoutMs) {
    if (existsSync(filePath)) {
      const size = statSync(filePath).size;
      if (size > 0 && size === lastSize && Date.now() - lastChangedAt >= 500) {
        return;
      }
      if (size !== lastSize) {
        lastSize = size;
        lastChangedAt = Date.now();
      }
    }
    await delay(100);
  }

  throw new Error(`Timed out waiting for OBS output to stabilize: ${filePath}`);
}

async function copyRecording(outputPath, demo, stamp) {
  if (!outputPath || !existsSync(outputPath)) return null;
  await waitForStableFile(outputPath);
  const extension = extname(outputPath) || ".mkv";
  const targetPath = join(rawDir, `${stamp}-${demo}${extension}`);
  if (resolve(outputPath) !== resolve(targetPath)) {
    copyFileSync(outputPath, targetPath);
  }
  return targetPath;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const spec = resolveSpec(options);
  const demoName = options.demoExplicit ? options.demo : demoNameFromSpec(spec);
  const stamp = timestamp();
  const manifestPath = join(manifestDir, `${stamp}-${demoName}.json`);
  const recordingDelayMs = Number(process.env.WDIO_RECORDING_DELAY_MS ?? "250");
  const readyFile = join(signalDir, `${stamp}-${demoName}.ready`);
  const goFile = join(signalDir, `${stamp}-${demoName}.go`);

  mkdirSync(rawDir, { recursive: true });
  mkdirSync(manifestDir, { recursive: true });
  mkdirSync(signalDir, { recursive: true });
  removeFile(readyFile);
  removeFile(goFile);

  const manifest = {
    demo: demoName,
    spec,
    startedAt: new Date().toISOString(),
    completedAt: null,
    obs: null,
    obsOutputPath: null,
    rawCopyPath: null,
    recorderGate: options.noRecord
      ? null
      : {
          readyFile,
          goFile,
        },
    success: false,
    error: null,
  };

  let obs = null;
  let recordingStarted = false;
  let wdioRun = null;

  try {
    if (!options.noRecord) {
      const connected = await connectObs({ openIfNeeded: options.openObs });
      obs = connected.client;
      const version = await obs.request("GetVersion");
      const recordStatus = await obs.request("GetRecordStatus");
      manifest.obs = {
        url: connected.url,
        obsVersion: version.obsVersion,
        obsWebSocketVersion: version.obsWebSocketVersion,
        rpcVersion: connected.identified.negotiatedRpcVersion,
      };

      if (recordStatus.outputActive) {
        throw new Error("OBS is already recording. Stop the current recording first.");
      }
    }

    if (!options.skipBuild) {
      await runCommand(
        "pnpm",
        ["tauri", "build", "--debug", "--no-bundle"],
        repoRoot,
        { VITE_WDIO_TAURI_PLUGIN: "1" },
      );
    }

    const wdioArgs = ["exec", "wdio", "run", "./wdio.conf.ts", "--spec", spec];

    if (obs) {
      wdioRun = spawnCommand("pnpm", wdioArgs, desktopDir, {
        SWARMDROP_DEMO_RECORDING: "1",
        SWARMDROP_DEMO_READY_FILE: readyFile,
        SWARMDROP_DEMO_GO_FILE: goFile,
      });

      await Promise.race([
        waitForFile(readyFile, 90_000),
        wdioRun.done.then(() => {
          throw new Error("WDIO exited before the demo was ready to record.");
        }),
      ]);

      await obs.request("StartRecord");
      recordingStarted = true;
      await delay(recordingDelayMs);
      writeFileSync(goFile, "go\n");
      await wdioRun.done;
    } else {
      await runCommand("pnpm", wdioArgs, desktopDir, {
        SWARMDROP_DEMO_RECORDING: "1",
      });
    }

    await delay(Math.max(200, Math.floor(recordingDelayMs / 2)));
    manifest.success = true;
  } catch (error) {
    if (
      wdioRun &&
      wdioRun.child.exitCode === null &&
      wdioRun.child.signalCode === null
    ) {
      wdioRun.child.kill("SIGTERM");
    }
    manifest.error = error instanceof Error ? error.message : String(error);
    process.exitCode = 1;
  } finally {
    if (obs && recordingStarted) {
      try {
        const stopResult = await obs.request("StopRecord");
        manifest.obsOutputPath = stopResult.outputPath ?? null;
        manifest.rawCopyPath = await copyRecording(
          manifest.obsOutputPath,
          demoName,
          stamp,
        );
      } catch (error) {
        manifest.success = false;
        manifest.error =
          error instanceof Error ? error.message : String(error);
        process.exitCode = 1;
      }
    }

    manifest.completedAt = new Date().toISOString();
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    removeFile(readyFile);
    removeFile(goFile);
    obs?.close();

    console.log(`Recording manifest: ${manifestPath}`);
    if (manifest.rawCopyPath) {
      console.log(`Recording copy: ${manifest.rawCopyPath}`);
    } else if (manifest.obsOutputPath) {
      console.log(`OBS output: ${manifest.obsOutputPath}`);
    }
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
