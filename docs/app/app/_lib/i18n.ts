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

/** 源 locale：msgid 就是中文原文，所以它永远不需要翻译目录也能正确显示。 */
export const SOURCE_LOCALE: Locale = "zh";

/** 用户显式选择的 locale 存这里；没有这一项表示「跟随浏览器」。 */
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
 * 两轮匹配：先精确（`zh-TW` → `zh-TW`），再按主语言（`zh-HK` / `zh-Hant` → `zh`）。
 * 一个都不沾则回退源 locale——**不猜**，英语用户在这里拿到的是中文，比拿到一个他也读不懂的
 * 第三种语言强。
 */
export function preferredLocale(languages: readonly string[]): Locale {
  for (const tag of languages) {
    if (isSupported(tag)) return tag;
  }
  for (const tag of languages) {
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
