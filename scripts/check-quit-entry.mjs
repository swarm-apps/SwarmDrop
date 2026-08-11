/**
 * 桌面前端终止进程（退出 / 重启）必须走 `src/lib/quit-app.ts`——它在杀进程前
 * `await flushTauriStores()`，把还在 IPC 路上的偏好写入送到后端（理由见该文件头部注释）。
 *
 * 这类「必须走某个封装」的约束靠人审守不住：绕过它的代码能编过、能跑、也不报错，
 * 只是用户下次启动时偏好没了——没有任何日志，且只在「同一个 tick 里写完就退」时触发。
 * 同形范式见 `check-clipboard-access.mjs`。
 *
 * ⚠️ **目前只有本地门禁**：`.github/workflows/` 下没有任何 workflow 跑前端检查
 * （`rust.yml` 只管 Rust），所以这条约束的实际执行者是 `/dev-workflow` skill 的
 * 门禁清单与人的自觉，不是 CI。
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const SRC_DIR = join(ROOT, "src");
/** 唯一允许直接触达进程终止 API 的文件。 */
const ENTRY_FILE = join("src", "lib", "quit-app.ts");

const FORBIDDEN = [
  {
    // `commands.quitApp()` —— tauri-specta 生成的退出命令
    pattern: /\bcommands\s*\.\s*quitApp\s*\(/,
    hint: "改调 quitApp() from src/lib/quit-app.ts",
  },
  {
    // `relaunch()` / `exit()` —— @tauri-apps/plugin-process
    pattern: /from\s+["']@tauri-apps\/plugin-process["']/,
    hint: "改调 relaunchApp() from src/lib/quit-app.ts",
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
  // bindings.ts 是 tauri-specta 生成物，命令定义本身不算调用
  return /\.(ts|tsx)$/.test(path) && !path.endsWith(join("src", "lib", "bindings.ts"));
}

/** 整行注释不算违规——封装文件与调用点要能在注释里解释为什么不直接用。 */
function isCommentLine(line) {
  const trimmed = line.trimStart();
  return (
    trimmed.startsWith("//") || trimmed.startsWith("*") || trimmed.startsWith("/*")
  );
}

const violations = [];

for (const path of walk(SRC_DIR).filter(isSourceFile)) {
  const file = relative(ROOT, path);
  if (file === ENTRY_FILE) continue;
  const lines = readFileSync(path, "utf8").split("\n");
  lines.forEach((line, index) => {
    if (isCommentLine(line)) return;
    for (const { pattern, hint } of FORBIDDEN) {
      if (pattern.test(line)) {
        violations.push(`${file}:${index + 1}: ${line.trim()}\n    → ${hint}`);
      }
    }
  });
}

if (violations.length > 0) {
  console.error("Found process-termination calls bypassing src/lib/quit-app.ts:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  console.error(
    "\n终止进程前必须 flush 偏好写入，否则刚改的偏好会随进程一起丢掉。",
  );
  process.exit(1);
}

console.log("Quit entry OK (all process termination goes through src/lib/quit-app.ts).");
