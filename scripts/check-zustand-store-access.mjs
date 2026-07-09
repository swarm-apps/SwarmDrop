import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const SRC_DIR = join(ROOT, "src");
const STORE_API_PATTERN =
  /use[A-Za-z0-9_]*Store\s*\.\s*(getState|setState)\s*\(/g;

const allowlist = [
  {
    file: "src/routes/index.tsx",
    pattern: /usePreferencesStore\s*\.\s*getState\s*\(/,
    reason: "router beforeLoad guard",
  },
  {
    file: "src/routes/_app/pairing.tsx",
    pattern: /useNetworkStore\s*\.\s*getState\s*\(/,
    reason: "router beforeLoad guard",
  },
  {
    file: "src/stores/transfer-store.ts",
    pattern: /useTransferStore\s*\.\s*getState\s*\(/,
    reason: "Tauri transfer event bridge",
  },
  {
    file: "src/stores/network-store.ts",
    pattern:
      /use(Network|Pairing|Secret|Preferences)Store\s*\.\s*(getState|setState)\s*\(/,
    reason: "network event bridge and lifecycle orchestration",
  },
  {
    file: "src/stores/secret-store.ts",
    pattern: /useSecretStore\s*\.\s*getState\s*\(/,
    reason: "store hydration helper outside React",
  },
  {
    file: "src/lib/device-name.ts",
    pattern: /use(Preferences|Network)Store\s*\.\s*(getState|setState)\s*\(/,
    reason: "synchronous device-name utility outside React",
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
