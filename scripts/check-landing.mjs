/**
 * 配对落地页（`docs/public/p/index.html`）的门禁：**体积**与**字典完整性**。
 *
 * 这一页刻意不走 Next（理由见页面头部与 `dev-notes/knowledge/web-app-frontend.md`），
 * 于是也吃不到任何工具链的兜底 —— Lingui 的 `Missing` 统计看不见 `public/`
 * （`docs/lingui.config.ts` 的 include 是 `app/app` 与 `../packages/file-browser/src`），
 * bundler 的体积报告也管不到它。这个脚本是它唯一的兜底。
 *
 * **一、体积 ≤ 10KB gzip。** 页面存在的全部理由是「慢网络下秒开」，但那是个形容词，
 * 没人会为它跑一次 gzip —— 它已经漂过一次：知识库长期记着「实测 3.2KB」，而页面后来长出
 * 深链出口与复制说明，悄悄涨到 5.9KB 都没被发现。10KB 的依据是 TCP 初始拥塞窗口：多数
 * 服务端 initcwnd = 10，10 × MSS(1460) ≈ 14.6KB，扣掉响应头后 10KB 的正文仍能在**第一个
 * RTT** 内送完，与页面「零额外请求」是同一个目标的两半。
 * ⚠️ `public/` 下的文件原样下发，所以那个文件里的**注释也在预算里**。
 *
 * **二、字典完整性。** 页面的多语言是「HTML 内置源 locale（zh）+ JS 字典只带 en / zh-TW」。
 * 这个形态的失败是静默的：漏译一条不报错，只让那一行**回落成中文**混在英文界面里 ——
 * 正是 Lingui 的 `Missing` 列在应用区拦住的那类问题。
 *
 * ⚠️ 与仓库其它前端检查一样，**目前只有本地门禁**：`.github/workflows/` 下没有跑前端
 * 检查的 workflow，执行者是 `/dev-workflow` 的门禁清单。
 */

import { gzipSync } from "node:zlib";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = process.cwd();
const PAGE = join(ROOT, "docs", "public", "p", "index.html");
const APP_I18N = join(ROOT, "docs", "app", "app", "_lib", "i18n.ts");
const LIMIT = 10 * 1024;

/**
 * 字典该有的语言 = 应用区的 `LOCALES` 减源 locale。
 *
 * **刻意手写而不是从 `i18n.ts` 推导**：这份独立清单正是门禁的价值 —— 从那边推导的话，
 * 「有人整块删掉了 en 字典」就变成无人看守。加第四种语言时这里会报错，那是要的。
 */
const LANGS = ["en", "zh-TW"];

const src = readFileSync(PAGE, "utf8");
const violations = [];

// ---- 一、体积 -------------------------------------------------------------
const raw = Buffer.byteLength(src);
const gzipped = gzipSync(src, { level: 9 }).byteLength;
const kb = (n) => `${(n / 1024).toFixed(1)}KB`;

if (gzipped > LIMIT) {
  violations.push(
    `体积 ${kb(gzipped)} gzip 超过 ${kb(LIMIT)} 上限（含注释）。` +
      `把内容挪进知识库，或改 LIMIT 并在提交信息里说明为什么这页值得多一次往返`,
  );
}

// ---- 二、字典完整性 -------------------------------------------------------
// 这个对象字面量里唯一的 `};` 就是它自己的结尾——内层两份字典后面跟的是 `,`。
// 页面是仓库自有文件、本身就是合法 JS，求值比手写解析器可靠。
const literal = /var STRINGS = (\{[\s\S]*?\n {8}\});/.exec(src);
if (!literal) throw new Error("找不到 `var STRINGS = {…};` —— 页面结构变了，先更新本脚本");
const dictionaries = new Function(`return ${literal[1]}`)();

// 页面上每个 `data-i18n` 与脚本里每个 `t("key", …)` 都得在两份字典里有着落。
const used = new Set([
  ...[...src.matchAll(/data-i18n="([^"]+)"/g)].map((m) => m[1]),
  ...[...src.matchAll(/\bt\(\s*"([^"]+)"/g)].map((m) => m[1]),
]);

for (const lang of LANGS) {
  // 整块缺失自然落进 missing，报出「少了 16 条」比「缺少整块」更具体。
  const dict = dictionaries[lang] ?? {};
  const missing = [...used].filter((key) => !dict[key]);
  const dead = Object.keys(dict).filter((key) => !used.has(key));

  if (missing.length) {
    violations.push(
      `\`${lang}\` 少了 ${missing.length} 条译文：${missing.join(", ")}` +
        `（漏译不报错，只会让这几行在 ${lang} 界面里回落成中文）`,
    );
  }
  if (dead.length) {
    violations.push(`\`${lang}\` 有 ${dead.length} 条没人用的死键：${dead.join(", ")}`);
  }
}

const extra = Object.keys(dictionaries).filter((lang) => !LANGS.includes(lang));
if (extra.length) {
  violations.push(`字典多了 ${extra.join(", ")} —— 语言集合变了就要同步更新本脚本的 LANGS`);
}

// 反向检查：**页面上每一处中文都得被 `data-i18n` 盖住**。
// 上面那条只保证「已标注的 key 两份字典齐」，新写一段中文却忘了标 `data-i18n`，
// 对门禁而言完全不存在 —— 那一行会在 en / zh-TW 界面里永远是中文。
// 只扫 `<body>` 到 `<script>` 之间的标记：脚本内部的中文（字典、`t()` 的默认值、注释）
// 本来就该是中文。
const markup = src
  .slice(src.indexOf("<body>"), src.indexOf("<script>"))
  .replace(/<!--[\s\S]*?-->/g, "")
  .replace(/<([a-z0-9]+)[^>]*\bdata-i18n="[^"]*"[^>]*>[\s\S]*?<\/\1>/gi, "");
const unmarked = markup.match(/[一-鿿][^<>]*/g);
if (unmarked) {
  violations.push(
    `这些中文没有 data-i18n，切语言时不会被替换：${unmarked
      .map((s) => `「${s.trim()}」`)
      .join("、")}`,
  );
}

// ---- 三、与应用区的两条跨文件契约 -----------------------------------------
// 落地页无法 import 应用区的任何东西（它不经构建），这两条只能靠核对维持。
const appSrc = readFileSync(APP_I18N, "utf8");

const storageKey = /const STORAGE_KEY = "([^"]+)"/.exec(appSrc);
if (!storageKey) {
  throw new Error("`_lib/i18n.ts` 的 STORAGE_KEY 形态变了，先更新本脚本");
}
if (!src.includes(`localStorage.getItem("${storageKey[1]}")`)) {
  violations.push(
    `落地页读的 localStorage key 与应用区的 STORAGE_KEY（${storageKey[1]}）对不上 —— ` +
      `不一致不会报错，只会让落地页永远忽略用户在应用区选过的语言`,
  );
}

// 这两个正则失配要 **throw 而不是跳过**：静默通过正是这条契约要防的失效形状本身。
const locales = /export const LOCALES = \[([^\]]+)\]/.exec(appSrc);
const source = /const SOURCE_LOCALE: Locale = "([^"]+)"/.exec(appSrc);
if (!locales || !source) {
  throw new Error("`_lib/i18n.ts` 的 LOCALES / SOURCE_LOCALE 形态变了，先更新本脚本");
}
const expected = [...locales[1].matchAll(/"([^"]+)"/g)]
  .map((m) => m[1])
  .filter((locale) => locale !== source[1]);
if ([...expected].sort().join() !== [...LANGS].sort().join()) {
  violations.push(
    `字典语言集合应为 ${expected.join(" / ")}（应用区 LOCALES 减源 locale），` +
      `实为 ${LANGS.join(" / ")}`,
  );
}

// ---- 结论 -----------------------------------------------------------------
if (violations.length) {
  console.error("配对落地页检查未通过：");
  for (const violation of violations) console.error(`- ${violation}`);
  console.error("\n判据与背景见本脚本头部注释。");
  process.exit(1);
}

console.log(
  `落地页检查通过：${kb(gzipped)} gzip / 上限 ${kb(LIMIT)}（raw ${kb(raw)}），` +
    `${used.size} 条文案 × ${LANGS.length} 份字典齐全`,
);
