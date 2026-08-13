/**
 * 节点启停**不许出现在 `useEffect` 里**。
 *
 * 禁的不是「组件调启停」——用户点「停止节点」按钮当然要调 `stopNetwork()`，那是事件
 * 处理器，一次点击一次调用。禁的是**响应式**地调：effect 随依赖重跑，而启停是一次性
 * 动作，两者对不上就会长出下面这个收敛环。
 *
 * 这条规则是用一个真实 bug 换来的：`src/routes/_app.tsx` 曾挂着
 *
 * ```tsx
 * useEffect(() => {
 *   if (autoStart && networkStatus === "stopped") void startNetwork();
 * }, [autoStart, networkStatus, startNetwork]);   // ← networkStatus 在依赖里
 * ```
 *
 * `stopNetwork()` 的最后一步正是 `set({ status: "stopped" })`，于是这个 effect 立刻把
 * 节点拉回去——**开关打开时「已停止」是一个不可达状态**，用户点停止只看到状态一闪就回。
 * 一个一次性的启动意图被写成了 `stopped → running` 的持续收敛环。
 *
 * 它还顺带让 `useNodeRestart.restart()` 一直在空转：stop 落地时 effect 抢先启动，
 * restart 自己那次撞上 `status === "starting"` 的幂等门禁直接返回 `true`。
 * **幂等门禁会掩盖这类竞态**——不红不报，只是让你以为编排是自己写的那份。
 *
 * 为什么是脚本而不是单测：这是一条**跨文件**的架构约束。写成「在某个 store 的单测里
 * `readFileSync` 一个路由文件 + 正则」看着能挡住回归，实际只覆盖那一个文件——同一个
 * 收敛环长在 `__root.tsx`、任意 `-components/` 或一个 hook 里都照绿，而注释会让人以为
 * 已经有护栏了。一个部分覆盖的护栏比没有护栏更糟，因为它会阻止别人去做完整的那个。
 *
 * ⚠️ **目前只有本地门禁**：`.github/workflows/` 下没有任何 workflow 跑前端检查
 * （`rust.yml` 只管 Rust），所以这条约束的实际执行者是 `/dev-workflow` 的门禁清单。
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";

const ROOT = process.cwd();
const SCAN_DIRS = [join(ROOT, "src"), join(ROOT, "docs", "app", "app")];

/**
 * 启停入口的各种叫法：桌面的 store action 与命令式边界、Web 端的运行时编排与底层原语。
 *
 * **漏一个名字 = 那条路径完全不设防**，而这个脚本的整个论点就是「部分覆盖的护栏比没有
 * 护栏更糟」。所以这里宁可多列：`stopNodeRuntime` 是 Web 端**唯一**的停止入口
 * （`_lib/node-lifecycle.ts`），`spawnNode` / `closeNode` 是它底下的原语
 * （`_lib/node-runtime.ts`）——早先这条正则只有 `closeNode`，于是一个
 * `useEffect(() => { void stopNodeRuntime(); }, [status])` 能带着绿灯合进去。
 */
const LIFECYCLE_CALL =
  /\b(startNetwork|stopNetwork|startNetworkFromStore|autoStartNodeIfEnabled|startNodeRuntime|stopNodeRuntime|spawnNode|closeNode)\s*\(/;

/**
 * **冷启动序列**：这些文件里 effect 内可以启停，但**依赖数组必须为空**——空依赖 = 挂载跑
 * 一次 = 一次性序列，正是本规则要求的形态。仍然扫描它们，只是把判据换成「依赖为空」。
 *
 * 整文件跳过是不行的：`src/main.tsx` 现在是唯一持有启动调用的地方，跳过它等于这条规则
 * 对最该看住的文件不生效——往那里加一个依赖 `networkStatus` 的 effect 就能把收敛环原样
 * 装回来，而门禁全绿。它自己的注释还写着「前端唯一不可能被单测覆盖的地方」。
 */
const COLD_START_FILES = [
  "src/main.tsx",
  "docs/app/app/_components/web-node-bootstrap.tsx",
  "docs/app/app/_lib/node-lifecycle.ts",
];

/**
 * 完全豁免，各有判据。每加一项都要能回答「它凭什么不会变成收敛环」。
 *
 * - `share-target.lazy.tsx` —— 外部「用 SwarmDrop 打开」的入口。它依赖 `status` 但有
 *   `startedRef` 一次性守卫，**且产品语义要求启动**：用户通过系统「打开方式」把文件交给
 *   本应用，那本身就是发送意图，此时节点没起就该起。它不受「自动启动节点」偏好约束
 *   （那个偏好管的是「开机要不要联网」，不是「我现在要发这个文件」），所以也不能改走
 *   `autoStartNodeIfEnabled()`。ref 守卫脚本判不了，这条只能靠人。
 */
const ALLOWLIST = ["src/routes/_app/send/share-target.lazy.tsx"];

function walk(dir) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return []; // 目录不存在时静默跳过（docs 是独立 workspace，可能没装）
  }
  return entries.flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      return entry.name === "node_modules" ? [] : walk(path);
    }
    return path;
  });
}

function isSourceFile(path) {
  return /\.(ts|tsx)$/.test(path) && !/\.test\.(ts|tsx)$/.test(path);
}

/**
 * 找出每个 `useEffect(` 调用覆盖的行区间。
 *
 * 括号计数足够：这些文件都过 tsc + prettier，不存在未闭合的括号；字符串里的裸括号
 * 会让计数偏移，但那要恰好出现在 effect 体内才可能误判，代价是一次误报而非漏报。
 */
function effectRanges(lines) {
  const ranges = [];
  for (let i = 0; i < lines.length; i += 1) {
    const start = /\buse(Layout)?Effect\(/.exec(lines[i])?.index ?? -1;
    if (start === -1) continue;
    let depth = 0;
    let started = false;
    for (let j = i; j < lines.length; j += 1) {
      const from = j === i ? start : 0;
      for (const ch of lines[j].slice(from)) {
        if (ch === "(") {
          depth += 1;
          started = true;
        } else if (ch === ")") {
          depth -= 1;
        }
      }
      if (started && depth <= 0) {
        // 依赖数组为空 = 挂载跑一次。闭合行形如 `}, []);`（prettier 保证）。
        const emptyDeps = /\}\s*,\s*\[\s*\]\s*\)/.test(lines[j]);
        ranges.push([i, j, emptyDeps]);
        i = j; // 同一个 effect 不再重复扫描
        break;
      }
    }
  }
  return ranges;
}

/** 整行注释不算违规——本次修复正是在原位留了一条指向新落点的注释。 */
function isCommentLine(line) {
  const trimmed = line.trimStart();
  return (
    trimmed.startsWith("//") ||
    trimmed.startsWith("*") ||
    trimmed.startsWith("/*")
  );
}

const violations = [];
let scanned = 0;

for (const dir of SCAN_DIRS) {
  for (const path of walk(dir).filter(isSourceFile)) {
    // 归一成 `/` 分隔：Windows 上 `relative()` 给的是 `src\\main.tsx`，两个清单里的
    // `src/main.tsx` 会全部失配 —— 结果是这道门在每个 Windows 开发者的干净工作树上
    // 都报错，而且恰恰是对着它最该保护的那个文件（main.tsx 的合法空依赖 effect）。
    const file = relative(ROOT, path).split(sep).join("/");
    if (ALLOWLIST.includes(file)) continue;
    const coldStart = COLD_START_FILES.includes(file);
    scanned += 1;
    const lines = readFileSync(path, "utf8").split("\n");
    for (const [from, to, emptyDeps] of effectRanges(lines)) {
      // 冷启动文件里，空依赖的 effect 是被允许的那一种；非空依赖仍然违规。
      if (coldStart && emptyDeps) continue;
      for (let i = from; i <= to; i += 1) {
        if (isCommentLine(lines[i])) continue;
        if (!LIFECYCLE_CALL.test(lines[i])) continue;
        const why = coldStart ? "（冷启动文件，但依赖数组非空）" : "";
        violations.push(`${file}:${i + 1}${why}: ${lines[i].trim()}`);
      }
    }
  }
}

if (violations.length > 0) {
  console.error("useEffect 里出现了节点启停：");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  console.error(
    "\neffect 随依赖重跑，而启停是一次性动作。写成 effect 会长出收敛环——" +
      "`stopNetwork()` 把状态置为 stopped，effect 立刻把节点拉回去，" +
      "用户根本停不掉。\n" +
      "用户触发的启停放事件处理器；冷启动时的一次性启动走 " +
      "`autoStartNodeIfEnabled()`（src/main.tsx 的启动序列）。",
  );
  process.exit(1);
}

console.log(`节点生命周期检查通过（扫了 ${scanned} 个文件，effect 内零启停调用）。`);
