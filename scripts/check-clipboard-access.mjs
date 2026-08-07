/**
 * 桌面前端禁止直接使用 WebView 的 Clipboard API——读写都必须走
 * `src/lib/clipboard.ts` 的 Tauri clipboard-manager 封装（理由见该文件头部注释）。
 *
 * 这类「必须走某个封装」的约束靠人审守不住：绕过它的代码能编过、能跑、
 * 只在某些平台静默失效。
 *
 * ⚠️ **目前只有本地门禁**：`.github/workflows/` 下没有任何 workflow 跑前端检查
 * （`rust.yml` 只管 Rust），所以这条约束的实际执行者是 `/dev-workflow` skill 的
 * 门禁清单与人的自觉，不是 CI。给前端加 CI 会改变所有 PR 的通过条件，是一个独立决策。
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const SRC_DIR = join(ROOT, "src");
// 只匹配调用形式，注释里提到 `navigator.clipboard` 这个名字本身不算违规
const CLIPBOARD_API_PATTERN = /navigator\s*\.\s*clipboard\s*\.\s*[A-Za-z]+\s*\(/;

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

/** 整行注释不算违规——封装文件本身要在注释里解释为什么不用这个 API。 */
function isCommentLine(line) {
  const trimmed = line.trimStart();
  return (
    trimmed.startsWith("//") ||
    trimmed.startsWith("*") ||
    trimmed.startsWith("/*")
  );
}

const violations = [];

for (const path of walk(SRC_DIR).filter(isSourceFile)) {
  const file = relative(ROOT, path);
  const lines = readFileSync(path, "utf8").split("\n");
  lines.forEach((line, index) => {
    if (isCommentLine(line)) return;
    if (!CLIPBOARD_API_PATTERN.test(line)) return;
    violations.push(`${file}:${index + 1}: ${line.trim()}`);
  });
}

if (violations.length > 0) {
  console.error("Found direct WebView Clipboard API usage:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  console.error(
    "\nUse copyText() / readText() from src/lib/clipboard.ts (Tauri clipboard-manager plugin).",
  );
  process.exit(1);
}

console.log("Clipboard access OK (no direct navigator.clipboard usage).");
