// Web 应用区的 i18n 运行时。
//
// 与桌面 `src/lib/i18n.ts` 同职责，但**约束完全不同**：这里是静态导出（`output: "export"`），
// 没有服务端，也不能按 locale 预生成路由——那会让路由数 ×3 并与 `basePath` 叠成更多子路径，
// 而这五条路由的内容全部是运行时数据，预生成不出任何东西。
//
// 所以 locale 是**纯客户端**决定的：显式选择 > 浏览器偏好 > 源 locale。

import { i18n, type Messages } from "@lingui/core";
import { messages as sourceMessages } from "../_locales/zh/messages";

/** 受支持的 locale，与桌面 `lingui.config.ts` 同一集合。 */
export const LOCALES = ["zh", "zh-TW", "en"] as const;

export type Locale = (typeof LOCALES)[number];

/**
 * 语言的**自称**（endonym）：给英语用户看 "English" 而不是「英语」，才认得出来。
 *
 * **刻意不走 Lingui。** 每种语言的名字要以**它自己**的形式出现，与界面当前是什么语言无关
 * ——切到英文时这一列仍然是「简体中文 / 繁體中文 / English」。翻译它等于把「用户能不能
 * 认出自己的母语」交给他此刻看不懂的那个 locale。
 *
 * 与 [`LOCALES`] 放在一起，是因为它们是同一件事的两半：加一种语言时这里必须同时长出一行，
 * `Record<Locale, string>` 会强制这一点。此前它在设置页与侧栏工具条各存了一份逐字相同的
 * 副本，两份都写着「与另一处同一份判断」——而它们不是一份。
 */
export const LOCALE_ENDONYM: Record<Locale, string> = {
  zh: "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
};

/**
 * 当前 locale 的自称，接受任意字符串。
 *
 * 存在的理由是 `useLocaleSwitcher().locale` 转发的是 lingui 的 `i18n.locale`——一个裸
 * `string`，类型上不保证落在 [`LOCALES`] 里。直接拿它索引 [`LOCALE_ENDONYM`] 要么被 tsc
 * 拦下，要么靠断言把「认不出来就崩」埋进调用点。这里统一回退源 locale 的名字。
 */
export function localeEndonym(locale: string): string {
  return LOCALE_ENDONYM[isSupported(locale) ? locale : SOURCE_LOCALE];
}

/** 源 locale：msgid 就是中文原文，所以它永远不需要翻译目录也能正确显示。 */
export const SOURCE_LOCALE: Locale = "zh";

/**
 * 用户显式选择的 locale 存这里；没有这一项表示「跟随浏览器」。
 *
 * ⚠️ **这把 key 有第二个读者，而它 import 不到本模块**：配对落地页
 * `docs/public/p/index.html` 是 `public/` 下的裸 HTML（不经构建，理由见那份文件的头部），
 * 只能硬编码同一个字面量。改名不会有任何编译错误，只会让落地页从此静默忽略用户选过的
 * 语言 —— `pnpm check:landing` 会核对这一条。
 *
 * 同一形状的还有 `swarmdrop:pending-invite`（sessionStorage，落地页写、
 * `_components/pairing-panel.tsx` 读）——**那把没有门禁**，只在知识库里记着。
 */
const STORAGE_KEY = "swarmdrop:locale";

/**
 * 源 locale 的目录**静态 import 并在模块加载时同步激活**。
 *
 * 这一条是静态导出的硬要求：预渲染发生在构建期，那一刻不能 await 任何东西，而
 * `i18n.locale` 未激活时 `<Trans>` 拿不到 catalog。静态 import + 同步激活让预渲染出来的
 * HTML 一定是完整的中文，JS 没加载出来时页面也不是空壳。
 *
 * 另外两个 locale 按需加载——它们只在用户实际切过去时才需要。
 */
i18n.loadAndActivate({ locale: SOURCE_LOCALE, messages: sourceMessages });

/**
 * 编译后的翻译目录。
 *
 * **显式列出三条而不是拼模板字符串**：三个 locale 而已，动态 import 的模板形式会让打包器
 * 生成一个 context 模块、把匹配到的所有文件都算进来，收益为零。
 *
 * `.ts` 由 `lingui compile --typescript` 从 `.po` 生成（见 package.json 的 `i18n:compile`），
 * **不入库**——它是产物，`postinstall` 与 `build` 都会重跑。
 */
const CATALOGS: Record<Locale, () => Promise<{ messages: Messages }>> = {
  zh: async () => ({ messages: sourceMessages }),
  "zh-TW": () => import("../_locales/zh-TW/messages"),
  en: () => import("../_locales/en/messages"),
};

function isSupported(value: string | null | undefined): value is Locale {
  return LOCALES.includes(value as Locale);
}

/** 读用户此前的显式选择。storage 不可用（隐私模式 / 服务端）时当作没选过。 */
export function storedLocale(): Locale | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return isSupported(value) ? value : null;
  } catch {
    return null;
  }
}

/**
 * 从浏览器语言偏好挑一个最接近的受支持 locale。
 *
 * **外层按用户的偏好顺序、内层才定匹配精度**（精确 → 繁体变体 → 主语言）。顺序不能反：
 * 此前是「先把整个列表精确扫一遍，再扫主语言」，于是靠后但精确的 tag 会压过靠前但不精确的
 * ——`["zh-CN", "en-US", "en"]`（Chrome 上中文用户最常见的一种列表）里 `en` 精确命中，
 * 首选简体中文的用户拿到的是**英文界面**。偏好顺序是用户表达的意图，精度只是我们的实现细节。
 *
 * 繁体那一档也不能省。繁体的写法远不止 `zh-TW`——`zh-HK` / `zh-MO` / `zh-Hant-*` 都是繁体，
 * 而按主语言归类会把它们全归进简体 `zh`。这种错法比「没匹配上」更难发现：它**匹配上了**，
 * 只是给了香港用户一屏简体字。
 *
 * 一个都不沾则回退源 locale。配对落地页（`docs/public/p/index.html`）有一份手写副本，
 * 除这最后一步（那边给 `en`，理由见那份文件）外**每个输入都必须同解**。
 */
export function preferredLocale(languages: readonly string[]): Locale {
  for (const tag of languages) {
    if (isSupported(tag)) return tag;

    const lower = tag.toLowerCase();
    // `yue`（粤语）一并按繁体处理：不认它就会落进主语言轮或源 locale，又是给港澳用户简体。
    if (lower.startsWith("zh") || lower.startsWith("yue")) {
      // `hans` 要先判：`-hk` / `-mo` 是**地区**不是字形，而 `zh-Hans-HK` 明说了简体。
      if (lower.includes("hans")) return "zh";
      return /hant|-hk|-mo|-tw|^yue/.test(lower) ? "zh-TW" : "zh";
    }

    const primary = tag.split("-")[0];
    if (isSupported(primary)) return primary;
  }
  return SOURCE_LOCALE;
}

/**
 * 解析本次应当使用的 locale：**显式选择优先于浏览器偏好**。
 *
 * 选过之后就该一直是那个——用户切到英文，不该因为系统语言是中文而在下次访问被改回去。
 */
export function resolveLocale(): Locale {
  return storedLocale() ?? preferredLocale(navigator.languages ?? [navigator.language]);
}

/** 加载并激活指定 locale。切换与首次激活走同一条路径。 */
export async function activateLocale(locale: Locale): Promise<void> {
  const { messages } = await CATALOGS[locale]();
  i18n.loadAndActivate({ locale, messages });
}

/** 记住用户的显式选择。storage 不可用时静默忽略——这一项丢了只是下次回到跟随浏览器。 */
export function rememberLocale(locale: Locale): void {
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // ignore
  }
}
