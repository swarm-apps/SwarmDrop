import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

// 本脚本有**两条互不相干的规则**，别把它们当成一回事：
//
//   规则 A（store API 访问）——禁止绕过 selector 直接 `useXStore.getState()/setState()`。
//     只对桌面 `src/` 有意义：那边 store 就叫 `useXStore`。Web 应用区的 store 是
//     `webNodeStore`（不以 use 开头），且 actions 本来就靠 `setState` 写，扫过去一条也匹配
//     不到、匹配到了也全是设计内用法。
//
//   规则 B（selector 派生）——禁止在 selector 里 map/filter 出新引用。
//     这条**两边都要**：Zustand 与 `useSyncExternalStore` 都用 `Object.is` 比较 selector
//     返回值，每次返回新数组就是每次快照不等 → 无限重渲染（"Maximum update depth exceeded"）。
//
// 历史注记：CLAUDE.md 与知识库一度把规则 A 描述成「防 selector 派生数组」的兜底——不是。
// 规则 B 此前根本不存在，两边都靠人审。Web 应用区从自研 store 迁到 zustand 时补上。
const ROOT = process.cwd();
const SRC_DIR = join(ROOT, "src");
/** 规则 B 的扫描范围：桌面 `src/` + Web 应用区。后者是独立 pnpm workspace，但脚本在根跑。 */
const SELECTOR_SCAN_DIRS = [SRC_DIR, join(ROOT, "docs", "app", "app")];
const STORE_API_PATTERN =
  /use[A-Za-z0-9_]*Store\s*\.\s*(getState|setState)\s*\(/g;

/** selector hook 的调用点：桌面的 `useXxxStore(...)` 与 Web 的 `useWebNode(...)`。 */
const SELECTOR_HOOK_PATTERN = /\buse(?:WebNode|[A-Za-z0-9_]*Store)\s*\(/g;

/** 会造出新引用的操作。命中后还要过一遍 SCALAR_TAIL 才算违规。 */
const DERIVING_CALL =
  /\.(map|filter|slice|concat|sort|reverse|flatMap|flat)\s*\(|Object\.(values|entries|keys|assign|fromEntries)\s*\(|Array\.from\s*\(/;

/**
 * 派生之后又收敛成标量的形态——放行。
 *
 * `Object.keys(s.offers).length` 与 `Object.values(s.projections).reduce(…, 0)` 都返回数字，
 * `Object.is(3, 3)` 为真，不会触发重渲染。这类写法在 app-nav 的徽标计数里是**正确**用法。
 */
const SCALAR_TAIL = /\.(length|size)\s*$|\.(reduce|some|every|includes|indexOf|find|findIndex|at)\s*\(/;

/**
 * 直接返回对象/数组字面量：`(s) => ({ a: s.a })` / `(s) => [s.a]`，必然是新引用。
 *
 * 对象字面量必须认 `(` 紧跟 `{` 这个形状，不能只看开头的 `(`——`(s) => (cond ? a : b)`
 * 这种括号包裹的表达式一样以 `(` 开头，但返回的是 store 里的原引用。
 */
const LITERAL_RETURN = /^\s*\(\s*\{|^\s*\[/;

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
    file: "src/stores/pairing-store.ts",
    pattern: /useSecretStore\s*\.\s*getState\s*\(/,
    // `previewInvite` 是所有粘贴路径的收口点，要在**出网之前**拦下自我邀请与已配对
    // 的邀请。两条判据（本机 id、已配对清单）都在 secret-store，而它是 store action
    // 不是组件——拿不到 hook。读的是一次性快照，不订阅。
    reason: "invite preview guards need an identity snapshot outside React",
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

/**
 * 从 `(` 处取到配对 `)` 之间的调用参数文本。
 *
 * 不解析字符串字面量里的括号——selector 里几乎不会出现，真出现了顶多是一次误报，
 * 比漏报安全。
 */
function extractCallArgs(content, openParenIndex) {
  let depth = 0;
  for (let i = openParenIndex; i < content.length; i += 1) {
    const ch = content[i];
    if (ch === "(") depth += 1;
    else if (ch === ")") {
      depth -= 1;
      if (depth === 0) return content.slice(openParenIndex + 1, i);
    }
  }
  return null;
}

/**
 * 取 selector 箭头函数的函数体；不是 `(s) => …` 形态就返回 null。
 *
 * 这顺带**放行了 `useXStore(useShallow((s) => ({…})))`**——参数是个调用而不是箭头函数，
 * 匹配不上就跳过。那正是想要的：`useShallow` 做浅比较，在它里面派生对象是安全的、也是
 * 桌面端的既定写法（`src/` 下 15 处）。看到它没报别当成漏检。
 */
function selectorBody(args) {
  const arrow = args.match(/^\s*\(?\s*[A-Za-z0-9_$]+\s*\)?\s*=>/);
  if (!arrow) return null;
  return args.slice(arrow[0].length).trim();
}

function derivesNewReference(body) {
  // 语句体（`(s) => { … }`）不做判断：里面可能有多条 return，正则分析必然误报。
  if (body.startsWith("{")) return false;
  if (LITERAL_RETURN.test(body)) return true;
  if (!DERIVING_CALL.test(body)) return false;
  return !SCALAR_TAIL.test(body);
}

const violations = [];
let allowedCount = 0;

// ── 规则 B：selector 派生 ────────────────────────────────────────────────────
const selectorFiles = new Set(
  SELECTOR_SCAN_DIRS.flatMap((dir) => walk(dir)).filter(isSourceFile),
);
for (const path of selectorFiles) {
  const file = relative(ROOT, path);
  const content = readFileSync(path, "utf8");
  for (const match of content.matchAll(SELECTOR_HOOK_PATTERN)) {
    const openParen = (match.index ?? 0) + match[0].length - 1;
    const args = extractCallArgs(content, openParen);
    if (args === null) continue;
    const body = selectorBody(args);
    if (body === null || !derivesNewReference(body)) continue;
    const oneLine = body.replace(/\s+/g, " ").slice(0, 80);
    violations.push(
      `${file}:${lineNumberFor(content, match.index ?? 0)}: selector 派生了新引用 → ${oneLine}`,
    );
  }
}

// ── 规则 A：store API 访问（仅桌面 src/）────────────────────────────────────
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
  console.error("Zustand 检查未通过：");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  console.error(
    "\nselector 只返回原始值或 store 内的稳定引用，派生放组件体内的 useMemo；" +
      "\nstore API 直接访问请在 React 组件里改用 useXStore(selector)，或给 allowlist 补一条边界理由。",
  );
  process.exit(1);
}

console.log(
  `Zustand 检查通过（selector 扫了 ${selectorFiles.size} 个文件，store API 放行 ${allowedCount} 处）。`,
);
