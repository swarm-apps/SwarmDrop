// 必须先于 @lingui/core —— Hermes 无 Intl.PluralRules,见 ./polyfills。
import "./polyfills";

import type { Messages } from "@lingui/core";
import { i18n } from "@lingui/core";
import {
  detectLanguage,
  type SupportedLanguage,
  saveLanguagePreference,
} from "./languageDetector";

const loaders: Record<
  SupportedLanguage,
  () => Promise<{ messages: Messages }>
> = {
  "zh-Hans": () => import("../locales/zh-Hans/messages.po"),
  en: () => import("../locales/en/messages.po"),
};

async function loadAndActivate(locale: SupportedLanguage): Promise<void> {
  const { messages } = await loaders[locale]();
  i18n.loadAndActivate({ locale, messages });
}

/** Bootstrap i18n at app start. */
export async function initI18n(): Promise<void> {
  const detected = await detectLanguage();
  await loadAndActivate(detected);
}

/** User explicitly picked a language. */
export async function setUserLanguage(
  locale: SupportedLanguage,
): Promise<void> {
  await saveLanguagePreference(locale);
  await loadAndActivate(locale);
}

/** User picked "follow system". */
export async function followSystemLanguage(): Promise<void> {
  await saveLanguagePreference("system");
  const detected = await detectLanguage();
  await loadAndActivate(detected);
}

export { i18n };
