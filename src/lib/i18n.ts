import { i18n } from "@lingui/core";

/**
 * 支持的语言列表
 * 添加新语言只需：
 * 1. 在此添加条目
 * 2. 在 lingui.config.ts 的 locales 数组中添加对应 key
 * 3. 运行 pnpm i18n:extract 生成 .po 文件
 * 4. 翻译 .po 文件
 */
export const locales = {
  zh: "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
};

export type LocaleKey = keyof typeof locales;

export const defaultLocale: LocaleKey = "zh";

/** 收窄任意字符串（如 `i18n.locale`，其类型只是 string）到受支持的语言。 */
export function isLocaleKey(value: string): value is LocaleKey {
  return value in locales;
}

/**
 * 动态加载并激活指定语言的消息目录
 * Vite 插件会自动编译 .po 文件
 */
export async function dynamicActivate(locale: LocaleKey) {
  const { messages } = await import(`../locales/${locale}/messages.po`);
  i18n.load(locale, messages);
  i18n.activate(locale);
}
