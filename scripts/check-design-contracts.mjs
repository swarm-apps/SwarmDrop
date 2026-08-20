/**
 * `DESIGN.md` 的跨端契约层不得消失，引用它的名字必须真的存在。
 *
 * ## 为什么需要机器门禁
 *
 * 2026-08-14 的 `e7d9caee` 在重新生成 `DESIGN.md` 时把整个契约层覆盖掉了
 * （1274 行 → 186 行，10 个 `### … Contract` 全没）。**六天没有人发现**——因为损害不是
 * 报错，而是 `CLAUDE.md` 与十几处代码注释里的引用**指向了不存在的东西**。读到那些注释的
 * 人（和 AI）会去 `DESIGN.md` 找判据，找不到，然后要么凭感觉实现、要么以为契约不存在。
 *
 * 这个文件分两层：视觉令牌层由 `/impeccable` 系列命令生成，**可以重新生成**；
 * `## Cross-platform Contracts` 是手写的跨端判据，**工具生成不出来**。本脚本守的是后者。
 *
 * ## 两条判据
 *
 * 1. **契约节数不低于 `MIN_CONTRACTS`**。整层被冲掉时它会归零，任何一次误删也会让它变小。
 * 2. **每一处引用都指得到**。扫全仓提到 `DESIGN.md` 的行，取出其中形如
 *    `Xxx Contract` 的名字，要求它是某个 `### ` 标题的前缀——于是
 *    `Node Status Contract` 对得上 `### Node Status Contract (cross-platform)`。
 *
 * ## 只管 `Contract`，不管 `Rule`
 *
 * 视觉令牌层里那些 `One Accent Rule` / `The Mono Truth Rule` 是**同一次重新生成**里被
 * 中文重写的（`**单一强调色规则**`），引用它们的注释因此也悬空着。那属于「token 层要不要
 * 保留英文命名」的问题，与本脚本要守的「手写判据别被工具冲掉」不是一回事——一起管会让
 * 这个门禁在第一次运行时就红，然后被人加进忽略清单。**先守住会被整层删除的那部分。**
 *
 * ⚠️ 与仓库其它前端检查一样，目前**只有本地门禁**：`.github/workflows/` 下没有跑它的
 * workflow，执行者是 `/dev-workflow` 的门禁清单。
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const DESIGN = join(ROOT, "DESIGN.md");

/**
 * 契约节数的下限。
 *
 * **合并或删除某一节是合法的**——那时连同这个数一起改，并在提交信息里说明。
 * 它拦的不是「少了一节」，是「整层没了」以及「顺手删掉一节而没人注意」。
 */
const MIN_CONTRACTS = 10;

/** 扫这些地方的引用。`dev-notes/archive` 是历史存档，指向旧节名是正常的。 */
const SCAN_DIRS = ["crates", "src", "src-tauri", "packages", "mobile/src", "docs/app", "scripts"];
const SCAN_FILES = ["CLAUDE.md", "AGENTS.md"];
const SKIP_DIRS = new Set(["node_modules", "target", "dist", ".next", "archive", "build"]);
const SCAN_EXTS = new Set([".rs", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".css", ".md"]);

/**
 * 引用名的形态：大写开头的英文词组 + `Contract`。
 *
 * 覆盖实际用过的每一种包裹方式，因为它们**只是措辞**：`**Device Card Contract**`、
 * `` `### Node Status Contract (cross-platform)` ``、以及最常见的裸名
 * （`DESIGN.md 的 Incoming Request Contract`）。所以不匹配包裹符号，只匹配名字本身。
 */
const REFERENCE = /\b([A-Z][A-Za-z]*(?: [A-Za-z][A-Za-z-]*)* Contract)\b/g;

/** 已知不指向 `DESIGN.md` 某一节的名字。初始为空——加进来之前先确认它真的不该存在。 */
const IGNORE = new Set();

function walk(dir, out) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out; // 目录不存在（比如没装 mobile 的工作副本）不是错误
  }
  for (const entry of entries) {
    if (entry.name.startsWith(".") || SKIP_DIRS.has(entry.name)) continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (SCAN_EXTS.has(entry.name.slice(entry.name.lastIndexOf(".")))) out.push(full);
  }
  return out;
}

const design = readFileSync(DESIGN, "utf8");

// —— 判据 1：契约层还在吗 ——
const headings = [...design.matchAll(/^### (.+)$/gm)].map((m) => m[1].trim());
const contracts = headings.filter((h) => h.includes("Contract"));
const problems = [];

if (contracts.length < MIN_CONTRACTS) {
  problems.push(
    `DESIGN.md 只剩 ${contracts.length} 个 "### … Contract" 小节，少于下限 ${MIN_CONTRACTS}。\n` +
      `  跨端契约层是手写的，工具生成不出来——它整层消失过一次（e7d9caee）。\n` +
      `  如果这次是有意合并或删除，请连同 scripts/check-design-contracts.mjs 的 MIN_CONTRACTS 一起改。`,
  );
}

// —— 判据 2：引用都指得到吗 ——
const files = [...SCAN_FILES.map((f) => join(ROOT, f)), ...SCAN_DIRS.flatMap((d) => walk(join(ROOT, d), []))];
const missing = new Map(); // 名字 -> 引用它的位置

for (const file of files) {
  let text;
  try {
    if (!statSync(file).isFile()) continue;
    text = readFileSync(file, "utf8");
  } catch {
    continue;
  }
  if (!text.includes("DESIGN.md")) continue;

  text.split("\n").forEach((line, i) => {
    if (!line.includes("DESIGN.md")) return;
    for (const [, name] of line.matchAll(REFERENCE)) {
      if (IGNORE.has(name)) continue;
      // 引用名是节标题的前缀即可——节标题常带 ` (cross-platform)` 这类限定后缀。
      if (headings.some((h) => h.startsWith(name))) continue;
      const where = `${relative(ROOT, file)}:${i + 1}`;
      missing.set(name, [...(missing.get(name) ?? []), where]);
    }
  });
}

for (const [name, places] of missing) {
  problems.push(
    `DESIGN.md 里没有「${name}」这一节，但有地方引用它：\n` +
      places.map((p) => `    ${p}`).join("\n") +
      `\n  要么补上那一节，要么把引用改成实际存在的节名。悬空引用不会报错，只会让读到它的人找不到判据。`,
  );
}

if (problems.length > 0) {
  console.error(`\n✗ DESIGN.md 契约检查未通过：\n\n${problems.join("\n\n")}\n`);
  process.exit(1);
}

console.log(`✓ DESIGN.md 契约层完好：${contracts.length} 节，引用全部指得到`);
