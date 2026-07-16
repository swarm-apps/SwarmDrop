#!/usr/bin/env node
// 把 e2e/desktop 录制的「原片 + 事件时间线」导入 Remotion 后期工程：
// - 原片 → video/public/demos/<demo>.mp4（strip 音轨，忽略提交）
// - 时间线 → video/src/demos/data/<demo>.timeline.json（去机器路径，可提交）
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const videoDir = resolve(scriptDir, "..");
const repoRoot = resolve(videoDir, "..");
const recDir = join(repoRoot, "e2e/desktop/build/desktop-recordings");
const demo = process.argv[2] ?? "desktop-home";

function latest(dir, suffix) {
  if (!existsSync(dir)) return null;
  const files = readdirSync(dir)
    .filter((f) => f.endsWith(suffix))
    .sort();
  return files.length ? join(dir, files[files.length - 1]) : null;
}

const timelinePath = latest(join(recDir, "timelines"), `-${demo}.json`);
if (!timelinePath) {
  throw new Error(`未找到 ${demo} 的 timeline：${recDir}/timelines（先跑 record:${demo}）`);
}
const timeline = JSON.parse(readFileSync(timelinePath, "utf8"));

const rawName = timeline.source?.path?.replace(/^raw\//, "");
const rawPath = rawName ? join(recDir, "raw", rawName) : latest(join(recDir, "raw"), `-${demo}.mov`);
if (!rawPath || !existsSync(rawPath)) {
  throw new Error(`未找到 ${demo} 原片：${rawPath}`);
}

const publicDemos = join(videoDir, "public", "demos");
const dataDir = join(videoDir, "src", "demos", "data");
mkdirSync(publicDemos, { recursive: true });
mkdirSync(dataDir, { recursive: true });

const mp4Out = join(publicDemos, `${demo}.mp4`);
// 无声 demo：strip 音轨。h264 直接转封装；失败则重编码。
try {
  execFileSync(
    "ffmpeg",
    ["-y", "-i", rawPath, "-an", "-c:v", "copy", "-movflags", "+faststart", mp4Out],
    { stdio: "ignore" },
  );
} catch {
  execFileSync(
    "ffmpeg",
    ["-y", "-i", rawPath, "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart", mp4Out],
    { stdio: "ignore" },
  );
}

const durationSec = execFileSync("ffprobe", [
  "-v",
  "error",
  "-show_entries",
  "format=duration",
  "-of",
  "default=nw=1:nk=1",
  mp4Out,
])
  .toString()
  .trim();
timeline.source.durationMs = Math.round(parseFloat(durationSec) * 1000);
timeline.source.path = `demos/${demo}.mp4`; // 去机器相关路径，只留 staticFile 相对名

const dataOut = join(dataDir, `${demo}.timeline.json`);
writeFileSync(dataOut, `${JSON.stringify(timeline, null, 2)}\n`);

console.log(`✓ ${demo}`);
console.log(`  原片 → ${mp4Out} (${timeline.source.durationMs}ms)`);
console.log(`  时间线 → ${dataOut} (${timeline.events.length} 事件)`);
