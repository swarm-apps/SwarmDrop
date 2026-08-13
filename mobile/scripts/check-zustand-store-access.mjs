import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const SRC_DIR = join(ROOT, "src");
const STORE_API_PATTERN =
  /use[A-Za-z0-9_]*Store\s*\.\s*(getState|setState)\s*\(/g;

const allowlist = [
  {
    file: "src/core/event-bus.ts",
    pattern:
      /use(MobileCore|Notification|Transfer|Inbox|Preferences)Store\s*\.\s*getState\s*\(/,
    reason: "native core event bridge",
  },
  {
    file: "src/core/foreign-file-access.ts",
    pattern: /useTransferStore\s*\.\s*getState\s*\(/,
    reason:
      "native file-access callback: publish byte reporting, called from Rust not React",
  },
  {
    file: "src/core/paths.ts",
    pattern: /usePreferencesStore\s*\.\s*getState\s*\(/,
    reason: "synchronous receive-path utility",
  },
  {
    file: "src/core/receive-location.ts",
    pattern: /usePreferencesStore\s*\.\s*getState\s*\(/,
    reason: "synchronous receive-location resolver, called from Rust publish path",
  },
  {
    file: "src/core/onboarding-flow.ts",
    pattern: /usePreferencesStore\s*\.\s*getState\s*\(/,
    reason: "synchronous onboarding-step derivation outside React",
  },
  {
    file: "src/components/device-organization-sheets.tsx",
    pattern: /usePreferencesStore\s*\.\s*getState\s*\(/,
    reason:
      "imperative present(): snapshot seeds local edit state; subscribing would clobber in-progress edits",
  },
  {
    file: "src/lib/device-name.ts",
    pattern: /use(Preferences|MobileCore)Store\s*\.\s*getState\s*\(/,
    reason: "synchronous device-name utility outside React",
  },
  {
    // ⚠️ store 名进了正则，所以**改 store 名要回来改这里**：这条原本写的是
    // `PairingCode`Store，6 位分享码换成 PairInvite 之后没跟着改，于是同一段
    // orchestration 代码一夜之间变成两条「违规」，而这个脚本当时零 CI 覆盖，
    // 谁都没看见。
    file: "src/stores/mobile-core-store.ts",
    pattern:
      /use(Preferences|PairingInvite)Store\s*\.\s*(getState|setState)\s*\(/,
    reason: "mobile core lifecycle orchestration",
  },
  {
    filePattern: /\.test\.(ts|tsx)$/,
    pattern: /use[A-Za-z0-9_]*Store\s*\.\s*(getState|setState)\s*\(/,
    reason: "test setup or assertion",
  },
];

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return walk(path);
    return path;
  });
}

function isSourceFile(path) {
  return /\.(ts|tsx)$/.test(path);
}

function lineNumberFor(content, index) {
  return content.slice(0, index).split("\n").length;
}

function allowed(file, matchText) {
  return allowlist.some((entry) => {
    if (entry.file && entry.file !== file) return false;
    if (entry.filePattern && !entry.filePattern.test(file)) return false;
    return entry.pattern.test(matchText);
  });
}

const violations = [];
let allowedCount = 0;

for (const path of walk(SRC_DIR).filter(isSourceFile)) {
  const file = relative(ROOT, path);
  const content = readFileSync(path, "utf8");
  for (const match of content.matchAll(STORE_API_PATTERN)) {
    const matchText = match[0];
    if (allowed(file, matchText)) {
      allowedCount += 1;
      continue;
    }
    violations.push(
      `${file}:${lineNumberFor(content, match.index ?? 0)}: ${matchText}`,
    );
  }
}

if (violations.length > 0) {
  console.error("Found non-allowlisted Zustand store API access:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  console.error(
    "\nUse useXStore(selector) in React components, or update the allowlist with a boundary reason.",
  );
  process.exit(1);
}

console.log(`Zustand store API access OK (${allowedCount} allowlisted).`);
