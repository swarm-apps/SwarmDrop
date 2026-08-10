#!/usr/bin/env node
/**
 * 护栏：禁止「pnpm patch 打在预编译包上」再次静默失效。
 *
 * ## 这条护栏为什么存在
 *
 * Expo SDK 56 起，绝大多数 expo-* 模块在 `node_modules/<pkg>/local-maven-repo/` 里随包
 * 附带一个**预编译 AAR**，autolinking 默认消费它（`shouldUsePublication = true` →
 * settings plugin 跳过 `linkProject()`），`node_modules/<pkg>/android/src` 下的 Kotlin
 * 源码**根本不参与构建**。
 *
 * pnpm 的 patch 改的正是 `node_modules` 里的源码。于是：
 *
 *   patch 应用成功（pnpm 不报错） + 源码确实被改了（grep 得到） + APK 里是旧行为
 *
 * ——三者同时成立，且**全程没有任何一处会报错**。2026-08 我们为此连续三个 commit、三次
 * 以为修好了 `expo-file-system` 的 SAF fd 泄漏，全是空的；直到 `javap` 拆开 AAR 才看见
 * 补丁加的字段压根不在产物里。
 *
 * 唯一的解法是在 `mobile/package.json` 声明
 * `expo.autolinking.android.buildFromSource: ["<gradle 项目名>"]` 强制从源码编译。
 * 而「有没有声明」这件事**没有任何现成检查会红**——这个脚本就是那个检查。
 *
 * Apple 侧是同一个坑的另一半：`expo-modules-autolinking/scripts/ios/precompiled_modules.rb`
 * 会用预编译 XCFramework 顶掉 `ios/` 目录，同样吃掉 patch，对应的开关是
 * `expo.autolinking.apple.buildFromSource`。
 *
 * ## 判据分两层
 *
 * **硬判据（红）**——只要 `pnpm install` 跑过就成立，不碰 JDK / gradle / Android SDK，
 * 所以能接在 PR 上（`.github/workflows/mobile-checks.yml`）而不是只在发版当天跑：
 *
 *   1. **平台归属**：只对「改动触及 `android/`」的 patch 跑 Android 检查，只对触及
 *      `ios/` 的跑 Apple 检查。纯 JS patch 不受预编译影响，强行要求 buildFromSource 是假阳。
 *   2. **配置门**：gradle 项目带 publication（= 随包发预编译产物）时，**无条件**要求它
 *      出现在对应平台的 `buildFromSource` 里。判定复刻各自的 gradle / CocoaPods 语义：
 *      Android 用**全匹配**正则对 **gradle 项目名**（`SettingsManager.kt` 的
 *      `Regex.matches(project.name)`），Apple 用**部分匹配**对 pod 名或 npm 包名
 *      （`precompiled_modules.rb` 的 `build_from_source?`）。
 *   3. **源码落地门**：patch 新增的符号必须真出现在 `node_modules` 的源文件里——否则
 *      是 pnpm patch 压根没应用，比预编译那个坑更靠前。
 *
 * **软诊断（只打印，不影响退出码）**——依赖 `javap` / `unzip`，缺了就跳过：
 *
 *   拆开预编译 AAR（以及 gradle 编过之后的 `<pkg>/android/build/`）看 patch 新增的符号
 *   在不在，用来回答「补丁到底进没进产物」。它是**排障线索**，不是判据。
 *
 * ### 配置门为什么必须是无条件的
 *
 * 它曾经挂在软诊断上：只有「符号在预编译 AAR 里缺失」才要求声明 buildFromSource。
 * 那样一来，**只改既有成员、不新增成员的 patch 会完全绕过护栏**——而那恰恰是护栏要防的
 * 那种静默失效。
 *
 * 例：patch 把 `fun read(length: Long)` 改成 `fun read(length: Long, offset: Long)`。
 * 符号比对是**名字级**的，AAR 里当然找得到 `read` ⇒「没有缺失」⇒ 配置门整条不执行。
 * 于是补丁 100% 不进 APK，脚本却 exit 0，还会打印一句方向完全相反的误导提示
 * 「预编译 AAR 已含 read，补丁可能已被上游合入」。
 *
 * 现在与 Apple 侧同姿态：**有预编译产物 + patch 动了该平台原生源码 ⇒ 必须声明**。
 * 对没有预编译产物的模块声明 buildFromSource 也是无害的（gradle 本来就从源码编译），
 * 所以这条宁可宽着要求，也不要留一个能被绕过的洞。
 *
 * ## 用法
 *
 *   node mobile/scripts/check-expo-patches.mjs     # 在任意目录都能跑
 *   pnpm -C mobile check:expo-patches
 *
 * CI 接在三处：PR / push 的 `mobile-checks.yml`（拦回归的主力）、`mobile-build-android.yml`
 * 与 `mobile-release.yml`（构建前的最后一道）。
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MOBILE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const WORKSPACE_YAML = path.join(MOBILE_ROOT, "pnpm-workspace.yaml");
const AUTOLINKING_BIN = path.join(MOBILE_ROOT, "node_modules", ".bin", "expo-modules-autolinking");

/** 收集到的失败原因；非空即 exit 1。 */
const failures = [];
/** 提示性信息，不影响退出码。 */
const notes = [];

const fail = (msg) => failures.push(msg);
const note = (msg) => notes.push(msg);

// ── 读配置 ────────────────────────────────────────────────────────────────────

/**
 * 只解析 `patchedDependencies:` 这一个顶层块映射，**刻意不引 `yaml` 包**。
 *
 * `yaml` 不在 `mobile/package.json` 里，脚本原先是靠依赖树提升到顶层才 require 得到它。
 * 上游哪天不再传递依赖它，这条护栏就会以一句「无法解析 yaml」红掉——一个与真因毫不
 * 相干的提示。而给 package.json 补一条显式依赖又要连带改 pnpm-lock.yaml 的 importers，
 * 代价比手写这几十行大。
 *
 * 换来的约束是：只认最朴素的块映射写法。任何看不懂的形态（流式 `{...}`、嵌套映射、
 * 锚点、多行标量）一律**抛错**，绝不退化成「解析出零个 patch」——后者是本脚本最怕的
 * 那种假通过（脚本绿、护栏其实没跑）。
 *
 * 返回 `null` 表示文件里根本没有这个键（合法的「零个 patch」）。
 */
function parsePatchedDependencies(text) {
  // YAML 里 `#` 只有在行首或空白之后才起注释作用，不能无脑 split("#")。
  const stripComment = (line) => {
    const m = /(^|\s)#/.exec(line);
    if (!m) return line;
    return line.slice(0, m.index + m[1].length);
  };

  const lines = text.split("\n");
  const result = {};
  let inBlock = false;
  let blockIndent = null;
  /** 注释里也写着 patchedDependencies（本仓就是），所以只统计**去注释后**的命中。 */
  let sawMentionOutsideBlock = false;

  for (let i = 0; i < lines.length; i++) {
    const line = stripComment(lines[i]).replace(/\s+$/, "");
    if (line.trim() === "") continue;
    const indent = line.length - line.trimStart().length;

    if (!inBlock) {
      if (indent !== 0) continue; // 其他顶层键的子项
      const m = /^patchedDependencies\s*:(.*)$/.exec(line);
      if (!m) {
        if (line.includes("patchedDependencies")) sawMentionOutsideBlock = true;
        continue;
      }
      if (m[1].trim() !== "") {
        throw new Error(
          `pnpm-workspace.yaml 第 ${i + 1} 行：patchedDependencies 不是块映射（${lines[i].trim()}），` +
            "本脚本只认最朴素的写法，请更新 parsePatchedDependencies()",
        );
      }
      inBlock = true;
      continue;
    }

    if (indent === 0) break; // 下一个顶层键，块结束
    if (blockIndent == null) blockIndent = indent;
    if (indent !== blockIndent) {
      throw new Error(
        `pnpm-workspace.yaml 第 ${i + 1} 行：patchedDependencies 下缩进不一致（多半是嵌套映射），` +
          "本脚本只认一层 `<包名@版本>: <patch 路径>`，请更新 parsePatchedDependencies()",
      );
    }

    const entry = /^\s*(?:"([^"]+)"|'([^']+)'|([^:]+?))\s*:\s*(?:"([^"]*)"|'([^']*)'|(.*))$/.exec(line);
    const key = entry ? (entry[1] ?? entry[2] ?? entry[3]) : null;
    const value = entry ? (entry[4] ?? entry[5] ?? entry[6]) : null;
    if (!key || !value) {
      throw new Error(
        `pnpm-workspace.yaml 第 ${i + 1} 行解析不出 \`<包名@版本>: <patch 路径>\`：${lines[i].trim()}`,
      );
    }
    result[key] = value;
  }

  if (!inBlock) {
    if (sawMentionOutsideBlock) {
      throw new Error(
        "pnpm-workspace.yaml 里出现了 patchedDependencies，但本脚本没能把它认成顶层块映射。" +
          "\n    不能当作「没有 patch」通过——请更新 parsePatchedDependencies()。",
      );
    }
    return null;
  }
  return result;
}

/**
 * `patchedDependencies` 住在 `mobile/pnpm-workspace.yaml`，**不在 package.json**
 * ——pnpm 11 已不读 package.json 的 `pnpm` 字段，读错地方会得到「零个 patch」这种
 * 最危险的假通过。
 */
function readPatchedDependencies() {
  if (!fs.existsSync(WORKSPACE_YAML)) {
    throw new Error(`找不到 ${WORKSPACE_YAML}`);
  }
  return parsePatchedDependencies(fs.readFileSync(WORKSPACE_YAML, "utf8")) ?? {};
}

/** `expo-file-system@56.0.8` → `expo-file-system`；`@scope/pkg@1.0.0` → `@scope/pkg`。 */
function packageNameOf(spec) {
  const at = spec.lastIndexOf("@");
  return at > 0 ? spec.slice(0, at) : spec;
}

/**
 * 权威的 buildFromSource 清单：**取自 autolinking resolve 的输出**，而不是脚本自己
 * 重解一遍 package.json。
 *
 * autolinking 的 `parsePackageJsonOptions` 做了两件本脚本曾经漏掉的事：把顶层
 * `expo.autolinking.buildFromSource` 与平台段合并（写在顶层同样生效），以及 apple 缺省
 * 回退到 ios。只看平台段的话语义更窄——声明在顶层就会被误判成「没声明」，红一个假阳。
 *
 * 注意 resolve 的输出在 buildFromSource 未配置时**不带** `configuration` 键。
 */
function declaredBuildFromSource(resolved) {
  const value = resolved?.configuration?.buildFromSource;
  return Array.isArray(value) ? value.filter((x) => typeof x === "string") : [];
}

// ── 解析 patch ────────────────────────────────────────────────────────────────

/**
 * 从 unified diff 里切出每个被改文件的新增行。
 * 返回 `[{ file, addedLines: [{ text, indent }] }]`，file 是包内相对路径。
 */
function parsePatch(patchText) {
  const files = [];
  let current = null;
  for (const line of patchText.split("\n")) {
    const header = /^\+\+\+ b\/(.+?)(?:\t.*)?$/.exec(line);
    if (header) {
      current = { file: header[1], addedLines: [] };
      files.push(current);
      continue;
    }
    // 只有 `+++ ` 需要显式跳过——它以 `+` 开头，会被下面那条「新增行」判据误收。
    // `--- ` 与 `diff --git` 本来就不以 `+` 开头，不必单列分支。
    if (line.startsWith("+++ ")) continue;
    if (current && line.startsWith("+")) {
      const text = line.slice(1);
      current.addedLines.push({ text, indent: text.length - text.trimStart().length });
    }
  }
  return files;
}

const KOTLIN_MODIFIER = String.raw`(?:private|internal|public|protected|override|open|final|lateinit|const|@\w+)`;
const KOTLIN_PROPERTY = new RegExp(String.raw`^((?:${KOTLIN_MODIFIER}\s+)*)(?:val|var)\s+(\w+)`);
const KOTLIN_FUNCTION = new RegExp(
  String.raw`^(?:${KOTLIN_MODIFIER}\s+)*(?:suspend\s+)?fun\s+(?:<[^>]*>\s*)?(\w+)\s*\(`,
);

/**
 * 从 Kotlin 新增行里解出**可被 javap 看见的类成员名**。
 *
 * 两条排除规则都是为了不造假阳：
 *   - 局部变量（`val name = e::class.java.simpleName`）没有可见性修饰符且缩进 ≥ 4，
 *     它不会出现在 javap 输出里，误收进来会让诊断满屏假缺失；
 *   - companion object 里的成员缩进 ≥ 4，编译进的是 `Xxx$Companion` 而不是外层类，
 *     所以 `fun` 也限制在缩进 ≤ 2 的顶层成员。
 */
function extractKotlinMembers(addedLines) {
  const members = new Set();
  for (const { text, indent } of addedLines) {
    const code = text.trim();
    if (code.startsWith("//") || code.startsWith("*") || code.startsWith("/*")) continue;

    const prop = KOTLIN_PROPERTY.exec(code);
    if (prop) {
      const hasModifier = prop[1].trim().length > 0;
      if (hasModifier || indent <= 2) members.add(prop[2]);
      continue;
    }
    const fn = KOTLIN_FUNCTION.exec(code);
    if (fn && indent <= 2) members.add(fn[1]);
  }
  return members;
}

/**
 * 平台归属门的判据：patch 里哪些文件属于该平台的原生源码。
 *
 * 这张表是**单一事实源**——「要不要为该平台跑检查」与「检查哪些文件」必须用同一份前缀，
 * 两处各写一份的话，加个前缀漏改一处就会静默跳过整个平台，正是本脚本要防的那类哑错误。
 */
const NATIVE_PREFIXES = {
  android: ["android/"],
  apple: ["ios/", "apple/"],
};

const nativeFilesOf = (patchFiles, platform) =>
  patchFiles.filter((f) => NATIVE_PREFIXES[platform].some((prefix) => f.file.startsWith(prefix)));

/**
 * 从 gradle 项目的 `sourceDir` 往上找到 npm 包根（`package.json` 的 name 对得上）。
 *
 * 不能图省事写 `path.dirname(sourceDir)`：那只在 gradle 项目正好放在 `<pkg>/android`
 * 时成立，而 `androidProjects()` 允许任意相对路径。patch 里的文件路径是相对 **npm 包根**
 * 的，根算错了就会去读一个不存在的文件——「patch 没生效」这条判据会静默跳过。
 */
function resolvePackageRoot(pkgName, startDir) {
  for (let dir = startDir; path.dirname(dir) !== dir; dir = path.dirname(dir)) {
    const manifest = path.join(dir, "package.json");
    if (!fs.existsSync(manifest)) continue;
    try {
      if (JSON.parse(fs.readFileSync(manifest, "utf8")).name === pkgName) return dir;
    } catch {
      // 坏的 package.json 不该让整条护栏挂掉，继续往上找。
    }
  }
  return null;
}

/** `android/src/main/java/a/b/C.kt` → `{ fqcn: "a.b.C", relPath: "a/b/C.class" }`。 */
function classOfKotlinFile(relFile) {
  const m = /(?:^|\/)src\/main\/(?:java|kotlin)\/(.+)\.kt$/.exec(relFile);
  if (!m) return null;
  const parts = m[1].split("/");
  return { fqcn: parts.join("."), relPath: `${m[1]}.class` };
}

// ── 外部命令 ──────────────────────────────────────────────────────────────────

function run(cmd, args, opts = {}) {
  return execFileSync(cmd, args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024, ...opts });
}

/** 探测某个命令在不在。缺了只影响软诊断，所以这里返回布尔而不是判红。 */
function hasTool(cmd) {
  try {
    run(cmd, ["-version"], { stdio: ["ignore", "pipe", "pipe"] });
    return true;
  } catch {
    try {
      run("command", ["-v", cmd], { shell: true, stdio: ["ignore", "pipe", "pipe"] });
      return true;
    } catch {
      return false;
    }
  }
}

/**
 * autolinking 的配置 loader 是**向上找最近的 package.json**，所以这条命令必须在
 * `mobile/` 下跑：在仓库根跑会命中根 package.json，静默返回空模块列表（实测过）。
 */
function resolveAutolinking(platform) {
  if (!fs.existsSync(AUTOLINKING_BIN)) {
    throw new Error(`找不到 ${AUTOLINKING_BIN}，请先在 mobile/ 下 \`pnpm install\``);
  }
  const out = run(AUTOLINKING_BIN, ["resolve", "-p", platform, "--json"], {
    cwd: MOBILE_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
  });
  return JSON.parse(out);
}

/** javap 出一个 class 的全部成员名（字段 + 方法）。class 不存在返回 null。 */
function javapMembers(classpath, fqcn) {
  let out;
  try {
    out = run("javap", ["-p", "-cp", classpath, fqcn], { stdio: ["ignore", "pipe", "pipe"] });
  } catch {
    return null;
  }
  const members = new Set();
  for (const line of out.split("\n")) {
    const decl = line.trim().replace(/;$/, "");
    if (!decl || decl.endsWith("{") || decl.startsWith("Compiled from")) continue;
    const method = /(?:^|[\s.])(\w+)\s*\(/.exec(decl);
    if (method) {
      members.add(method[1]);
      // Kotlin 属性会编成 getX/setX，回填成属性名，方便与源码里的 `val x` 对齐。
      const accessor = /^(?:get|set)([A-Z]\w*)$/.exec(method[1]);
      if (accessor) members.add(accessor[1][0].toLowerCase() + accessor[1].slice(1));
      continue;
    }
    const field = /(\w+)$/.exec(decl);
    if (field) members.add(field[1]);
  }
  return members;
}

/** 从 publication 描述反推随包附带的 AAR 路径。 */
function locateAar(pkgRoot, publication) {
  const { groupId, artifactId, version, repository } = publication;
  if (repository !== "local-maven-repo") return { path: null, repository };
  const guess = path.join(
    pkgRoot,
    "local-maven-repo",
    ...groupId.split("."),
    artifactId,
    version,
    `${artifactId}-${version}.aar`,
  );
  return { path: fs.existsSync(guess) ? guess : null, repository };
}

/**
 * gradle 编过之后的 class 产物（`<pkg>/android/build/**`）。返回**全部**命中，按路径排序。
 *
 * 刻意不「返回第一个命中」：`build/` 下常同时躺着 debug / release 等多个变体，谁先被
 * `readdir` 到取决于目录遍历顺序，据此下结论是**不确定的**；而且旧产物不会自动清理，
 * 一份陈旧的 class 足以让结论方向完全反过来。所以这里只列出、只作诊断，绝不据此判红
 * ——判红的是配置门。
 */
function locateBuiltClasses(sourceDir, relClassPath) {
  const buildDir = path.join(sourceDir, "build");
  if (!fs.existsSync(buildDir)) return [];
  const suffix = path.sep + relClassPath.split("/").join(path.sep);
  const found = [];
  const stack = [buildDir];
  while (stack.length > 0) {
    const dir = stack.pop();
    let entries;
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (full.endsWith(suffix)) {
        // classpath 根 = 去掉包路径后的那一截
        found.push({ classFile: full, classpathRoot: full.slice(0, full.length - suffix.length + 1) });
      }
    }
  }
  return found.sort((a, b) => a.classFile.localeCompare(b.classFile));
}

// ── 检查 ──────────────────────────────────────────────────────────────────────

function checkAndroid({ pkgName, patchFiles, resolved, declared, tmpDir, javapAvailable }) {
  const androidFiles = nativeFilesOf(patchFiles, "android");
  if (androidFiles.length === 0) return;

  const label = `[android] ${pkgName}`;
  const module = resolved.modules.find((m) => m.packageName === pkgName);
  if (!module) {
    note(`${label}：不是 autolinking 识别的 Expo 模块，预编译 AAR 的坑不适用，跳过。`);
    return;
  }

  const allProjects = module.projects ?? [];
  const pkgRoot = allProjects.length > 0 ? resolvePackageRoot(pkgName, allProjects[0].sourceDir) : null;

  // ── 待断言的符号 ──
  // 只由 patch 内容决定，与具体 gradle 项目无关，所以在进 projects 循环之前算一次。
  const inspected = [];
  for (const file of androidFiles) {
    if (!file.file.endsWith(".kt")) continue;
    const members = extractKotlinMembers(file.addedLines);
    if (members.size === 0) continue;
    inspected.push({ klass: classOfKotlinFile(file.file), members, relFile: file.file });
  }

  // ── 硬判据：源码落地门 ──
  // 与预编译无关的更靠前一层：patch 有没有真的落到 node_modules 的源文件上。
  if (pkgRoot == null && allProjects.length > 0) {
    fail(`${label}：从 ${allProjects[0].sourceDir} 往上找不到 ${pkgName} 的包根，无法核对 patch 是否落地。`);
  }
  if (pkgRoot) {
    for (const { members, relFile } of inspected) {
      const sourceFile = path.join(pkgRoot, ...relFile.split("/"));
      if (!fs.existsSync(sourceFile)) {
        // patch 声明改了它、它却不在——要么 patch 过期，要么包根算错了。两种都不能放过。
        fail(`${label}：patch 声明改了 ${relFile}，但 ${path.relative(MOBILE_ROOT, sourceFile)} 不存在。`);
        continue;
      }
      const src = fs.readFileSync(sourceFile, "utf8");
      const notInSource = [...members].filter((m) => !new RegExp(`\\b${m}\\b`).test(src));
      if (notInSource.length > 0) {
        fail(
          `${label} / ${relFile}：node_modules 里的源码不含 ${notInSource.join("、")}` +
            ` ⇒ pnpm patch 没有生效，先跑 \`pnpm -C mobile install\`。`,
        );
      }
    }
  }

  // ── 硬判据：配置门 ──
  const projects = allProjects.filter((p) => p.publication != null);
  if (projects.length === 0) {
    note(`${label}：该模块不发预编译产物（无 publication），gradle 恒从源码编译，patch 天然生效。`);
    return;
  }

  // 复刻 SettingsManager.kt：`buildFromSourceRegex.any { it.matches(project.name) }`，
  // Kotlin 的 Regex.matches 是**全匹配**，这里必须同样全匹配，否则本地判定会比 gradle 宽。
  const matchesDeclared = (name) =>
    declared.some((pattern) => {
      try {
        return new RegExp(`^(?:${pattern})$`).test(name);
      } catch {
        fail(`${label}：expo.autolinking[.android].buildFromSource 里 "${pattern}" 不是合法正则`);
        return false;
      }
    });

  for (const project of projects) {
    if (matchesDeclared(project.name)) {
      note(`${label} / gradle 项目 ${project.name}：已声明 buildFromSource ✓`);
      continue;
    }
    fail(
      `${label} / gradle 项目 ${project.name}：patch 改了 ${androidFiles.map((f) => f.file).join("、")}，` +
        `但该模块随包发预编译 AAR、gradle 默认消费它，补丁**不会进构建**。\n` +
        `    修法：mobile/package.json 加\n` +
        `      "expo": { "autolinking": { "android": { "buildFromSource": ["${project.name}"] } } }\n` +
        `    注意 buildFromSource 匹配的是 **gradle 项目名**（全匹配正则），不是 npm 包名。\n` +
        `    这条判据是无条件的：只改既有成员、不新增成员的 patch 在产物里看不出差别，` +
        `靠符号比对根本拦不住（见文件头注释）。`,
    );
  }

  // ── 软诊断：拆产物看符号 ──
  if (!javapAvailable) {
    note(`${label}：找不到 \`javap\`（JDK 自带），跳过产物符号诊断。配置门不依赖它，判红不受影响。`);
    return;
  }
  if (inspected.length === 0) {
    note(
      `${label}：patch 没有可解析的 Kotlin 新增成员（例如只改了 android/build.gradle），` +
        `跳过产物符号诊断——这类改动同样被预编译 AAR 100% 忽略，由上面的配置门看守。`,
    );
    return;
  }

  for (const project of projects) {
    // local-maven-repo 挂在 npm 包根下，不是 gradle 项目目录下。
    const { path: aarPath, repository } = locateAar(pkgRoot ?? path.dirname(project.sourceDir), project.publication);
    if (!aarPath) {
      note(
        `${label} / gradle 项目 ${project.name}：定位不到随包 AAR（repository=${repository}），` +
          `跳过产物符号诊断。想恢复诊断请扩展 locateAar()。`,
      );
      continue;
    }

    // classes.jar 是整个 AAR 一份、与具体类无关，拆一次给下面所有类共用。
    const jarPath = path.join(tmpDir, `${project.name}-classes.jar`);
    try {
      fs.writeFileSync(jarPath, run("unzip", ["-p", aarPath, "classes.jar"], { encoding: "buffer" }));
    } catch {
      note(`${label}：无法从 ${path.relative(MOBILE_ROOT, aarPath)} 里取出 classes.jar（需要 \`unzip\`），跳过产物符号诊断。`);
      continue;
    }

    for (const { klass, members, relFile } of inspected) {
      if (!klass) {
        note(`${label} / ${relFile}：不在 src/main/{java,kotlin} 下，推不出类名，跳过产物符号诊断。`);
        continue;
      }
      const wanted = [...members].map((m) => `\`${m}\``).join("、");
      const aarMembers = javapMembers(jarPath, klass.fqcn);
      if (aarMembers == null) {
        note(`${label} / ${klass.fqcn}：javap 在预编译 AAR 里找不到这个类（可能是新增类），跳过对比。`);
      } else {
        const missing = [...members].filter((m) => !aarMembers.has(m));
        note(
          missing.length > 0
            ? `${label} / ${klass.fqcn}：预编译 AAR 里缺 ${missing.map((m) => `\`${m}\``).join("、")}` +
                ` ⇒ 消费 AAR 就等于没打这条补丁（已由配置门强制走源码）。`
            : `${label} / ${klass.fqcn}：预编译 AAR 已含 ${wanted}。注意符号比对只到**名字级**，` +
                `签名/实现的改动看不出来，别据此断定补丁多余。`,
        );
      }

      // gradle 编过就顺带看一眼真正被消费的产物。多变体 / 陈旧产物都可能存在，
      // 所以全部列出、只作线索。
      for (const built of locateBuiltClasses(project.sourceDir, klass.relPath)) {
        const builtMembers = javapMembers(built.classpathRoot, klass.fqcn);
        const missingBuilt = builtMembers == null ? [...members] : [...members].filter((m) => !builtMembers.has(m));
        const rel = path.relative(MOBILE_ROOT, built.classFile);
        note(
          missingBuilt.length > 0
            ? `${label} / ${klass.fqcn}：gradle 产物 ${rel} 里缺 ${missingBuilt.join("、")}` +
                `（可能是上一轮的陈旧产物，\`./gradlew clean\` 后重编再看）。`
            : `${label} / ${klass.fqcn}：gradle 产物 ${rel} 已含全部补丁符号 ✓`,
        );
      }
    }
  }
}

function checkApple({ pkgName, patchFiles, resolvedApple, declared }) {
  const appleFiles = nativeFilesOf(patchFiles, "apple");
  if (appleFiles.length === 0) return;

  const label = `[apple] ${pkgName}`;
  const module = resolvedApple.modules.find((m) => m.packageName === pkgName);
  if (!module) {
    note(`${label}：不是 autolinking 识别的 Expo 模块，预编译 XCFramework 的坑不适用，跳过。`);
    return;
  }

  const podNames = (module.pods ?? []).map((p) => p.podName).filter(Boolean);
  const candidates = [pkgName, ...podNames];
  // 复刻 precompiled_modules.rb 的 build_from_source?：Ruby 的 match? 是**部分匹配**，
  // 且同时拿 pod 名与 npm 包名去试。这里不要收紧成全匹配，否则会比 CocoaPods 侧更严。
  const matched = declared.some((pattern) => {
    try {
      const re = new RegExp(pattern);
      return candidates.some((name) => re.test(name));
    } catch {
      fail(`${label}：expo.autolinking[.apple].buildFromSource 里 "${pattern}" 不是合法正则`);
      return false;
    }
  });

  if (!matched) {
    fail(
      `${label}：patch 改了 ${appleFiles.map((f) => f.file).join("、")}，但 Expo 模块在 pod install 时可能被` +
        `预编译 XCFramework 顶掉（precompiled_modules.rb），补丁会被静默吃掉。\n` +
        `    修法：mobile/package.json 加\n` +
        `      "expo": { "autolinking": { "apple": { "buildFromSource": ["${pkgName}"] } } }\n` +
        `    （声明了强制走源码，对没有预编译产物的模块也是无害的。）`,
    );
  } else {
    note(`${label}：已声明 buildFromSource ✓`);
  }
}

// ── 主流程 ────────────────────────────────────────────────────────────────────

function main() {
  const patched = readPatchedDependencies();
  const specs = Object.keys(patched);
  if (specs.length === 0) {
    console.log("check-expo-patches: pnpm-workspace.yaml 里没有 patchedDependencies，无需检查。");
    return 0;
  }

  // 先把每个 patch 解出来，据此决定要不要跑（较慢的）autolinking resolve。
  const entries = [];
  for (const spec of specs) {
    const pkgName = packageNameOf(spec);
    const patchPath = path.resolve(MOBILE_ROOT, patched[spec]);
    if (!fs.existsSync(patchPath)) {
      fail(`[${pkgName}]：pnpm-workspace.yaml 指向的 patch 文件不存在：${patched[spec]}`);
      continue;
    }
    const patchFiles = parsePatch(fs.readFileSync(patchPath, "utf8"));
    entries.push({ spec, pkgName, patchFiles });
  }

  const needAndroid = entries.some((e) => nativeFilesOf(e.patchFiles, "android").length > 0);
  const needApple = entries.some((e) => nativeFilesOf(e.patchFiles, "apple").length > 0);

  let tmpDir = null;
  try {
    if (needAndroid) {
      const resolved = resolveAutolinking("android");
      // javap / unzip 只服务软诊断——缺了照样要把配置门跑完，那才是能拦住回归的判据。
      const javapAvailable = hasTool("javap");
      tmpDir = javapAvailable ? fs.mkdtempSync(path.join(os.tmpdir(), "check-expo-patches-")) : null;
      const declared = declaredBuildFromSource(resolved);
      for (const entry of entries) {
        checkAndroid({ ...entry, resolved, declared, tmpDir, javapAvailable });
      }
    }
    if (needApple) {
      const resolvedApple = resolveAutolinking("apple");
      const declared = declaredBuildFromSource(resolvedApple);
      for (const entry of entries) {
        checkApple({ ...entry, resolvedApple, declared });
      }
    }
  } finally {
    if (tmpDir) fs.rmSync(tmpDir, { recursive: true, force: true });
  }

  for (const n of notes) console.log(`  · ${n}`);

  if (failures.length > 0) {
    console.error(`\ncheck-expo-patches: ${failures.length} 项不通过\n`);
    for (const f of failures) console.error(`  ✗ ${f}\n`);
    console.error("背景：pnpm patch 打在预编译 AAR / XCFramework 上会**静默失效**——");
    console.error("      patch 应用成功、源码确实被改、APK 里却是旧行为，全程零报错。");
    return 1;
  }

  console.log(`\ncheck-expo-patches: ${entries.length} 个 patch 全部通过 ✓`);
  return 0;
}

try {
  process.exit(main());
} catch (error) {
  console.error(`check-expo-patches: ${error.message}`);
  process.exit(1);
}
