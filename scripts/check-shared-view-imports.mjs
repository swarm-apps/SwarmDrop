/**
 * `packages/shared-view` 必须零平台依赖：不许 import React / React Native / DOM 垫片 /
 * Node 内置 / 任一端的 bindings（`@/lib/bindings`、`swarmdrop-web`、
 * `react-native-swarmdrop-core`）。
 *
 * 判据取的是**比清单更强的形式**：非测试源文件里，只允许相对路径 import。
 * 白名单会随生态变长而漏（今天列了 react，明天有人 import 一个包着 DOM 的工具库），
 * 「一个外部包都不许有」则不会。包本身的 package.json 也确实没有任何 dependency——
 * 本脚本是让这件事在**代码里**也成立。
 *
 * **为什么不能只靠 tsconfig**：`lib: ["ES2022"]`（不含 DOM）能挡住 `document.` / `window.`，
 * 但挡不住 `import { useState } from "react"` —— 包嵌在仓库根之下，tsc 的模块解析会一路
 * 向上走到根 `node_modules` 并解析成功。实测确认过，别以为 pnpm 的 isolated 链接兜得住。
 *
 * ⚠️ 与仓库其它前端检查一样，**目前只有本地门禁**：`.github/workflows/` 下没有跑前端
 * 检查的 workflow，执行者是 `/dev-workflow` 的门禁清单。
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const PKG_DIR = join(ROOT, "packages", "shared-view", "src");

/** 测试文件可以 import 测试框架——它不进产物，也不会被三端消费。 */
const TEST_ONLY_ALLOWED = new Set(["vitest"]);

/**
 * 静态 `import` / `export … from` / 动态 `import()` / `require()` 四种形式的 specifier。
 * 逐行扫描（与仓库其它 check 脚本一致，不引 AST 依赖）；本包代码风格统一，
 * import 语句都在单行内。
 */
const SPECIFIER_PATTERNS = [
  /^\s*import\s+(?:type\s+)?[^'"]*from\s*['"]([^'"]+)['"]/,
  /^\s*import\s*['"]([^'"]+)['"]/,
  /^\s*export\s+(?:type\s+)?[^'"]*from\s*['"]([^'"]+)['"]/,
  /\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)/,
  /\brequire\s*\(\s*['"]([^'"]+)['"]\s*\)/,
];

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? walk(path) : path;
  });
}

const isSourceFile = (path) => /\.tsx?$/.test(path);
const isTestFile = (path) => /\.test\.tsx?$/.test(path);
const isRelative = (specifier) => specifier.startsWith("./") || specifier.startsWith("../");

const violations = [];

for (const path of walk(PKG_DIR).filter(isSourceFile)) {
  const file = relative(ROOT, path);
  const allowBare = isTestFile(path);
  readFileSync(path, "utf8")
    .split("\n")
    .forEach((line, index) => {
      for (const pattern of SPECIFIER_PATTERNS) {
        const specifier = pattern.exec(line)?.[1];
        if (specifier === undefined) continue;
        if (isRelative(specifier)) return;
        if (allowBare && TEST_ONLY_ALLOWED.has(specifier)) return;
        violations.push(`${file}:${index + 1}: ${specifier}`);
        return;
      }
    });
}

if (violations.length > 0) {
  console.error("packages/shared-view imported something outside itself:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  console.error(
    "\nThe package must stay platform-neutral: relative imports only " +
      "(test files may additionally import vitest). " +
      "Platform-bound logic belongs in src/ · docs/app/app · mobile/src, not here.",
  );
  process.exit(1);
}

console.log("shared-view imports OK (relative only, no platform dependencies).");
