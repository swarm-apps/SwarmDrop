#!/usr/bin/env node
import { mkdirSync, rmSync } from "node:fs";
import { spawn, spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const rawDir = resolve(scriptDir, "../build/desktop-recordings/raw");
const args = process.argv.slice(2).filter((arg) => arg !== "--");
const platform = args[0] ?? "ios";
const duration = args[1] ? Number(args[1]) : null;

if (platform === "help" || platform === "--help" || platform === "-h") {
  console.log("Usage: pnpm --dir e2e/desktop record:mobile [ios|android]");
  process.exit(0);
}

if (platform !== "ios" && platform !== "android") {
  throw new Error("Platform must be ios or android");
}
if (args.length > 2 || (duration !== null && (!Number.isInteger(duration) || duration <= 0))) {
  throw new Error("Usage: record:mobile [ios|android] [seconds]");
}

mkdirSync(rawDir, { recursive: true });
const output = join(
  rawDir,
  `${platform}-${new Date().toISOString().replace(/[:.]/g, "-")}.mp4`,
);
const androidSerial = process.env.ANDROID_SERIAL ?? "emulator-5554";
const androidRemotePath = "/sdcard/swarmdrop-recording.mp4";

function run(command, commandArgs, seconds) {
  const child = spawn(command, commandArgs, { stdio: "inherit" });
  return new Promise((resolveDone, rejectDone) => {
    const stop = () => child.kill("SIGINT");
    const timer = seconds ? setTimeout(stop, seconds * 1_000) : null;
    process.once("SIGINT", stop);
    child.once("error", (error) => {
      if (timer) clearTimeout(timer);
      process.removeListener("SIGINT", stop);
      rejectDone(error);
    });
    child.once("exit", (code, signal) => {
      if (timer) clearTimeout(timer);
      process.removeListener("SIGINT", stop);
      if (code === 0 || signal === "SIGINT") resolveDone();
      else rejectDone(new Error(`${command} exited with code=${code} signal=${signal}`));
    });
  });
}

async function main() {
  rmSync(output, { force: true });
  console.log(`Recording ${platform} to ${output}`);
  console.log(duration ? `Stopping after ${duration} seconds.` : "Press Ctrl+C to stop recording.");

  if (platform === "ios") {
    await run("xcrun", [
      "simctl",
      "io",
      "booted",
      "recordVideo",
      "--display=1",
      "--codec=h264",
      "--force",
      output,
    ], duration);
    return;
  }

  await run("adb", [
    "-s",
    androidSerial,
    "shell",
    "screenrecord",
    ...(duration ? ["--time-limit", String(duration)] : []),
    androidRemotePath,
  ]);
  spawnSync("adb", ["-s", androidSerial, "pull", androidRemotePath, output], {
    stdio: "inherit",
  });
  spawnSync("adb", ["-s", androidSerial, "shell", "rm", androidRemotePath]);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
