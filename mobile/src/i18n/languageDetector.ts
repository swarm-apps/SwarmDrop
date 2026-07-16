import AsyncStorage from "@react-native-async-storage/async-storage";
import { getLocales } from "expo-localization";

export const SUPPORTED_LANGUAGE_CODES = ["zh-Hans", "en"] as const;
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGE_CODES)[number];

export interface LanguageInfo {
  code: SupportedLanguage;
  nativeName: string;
  englishName: string;
}

export const SUPPORTED_LANGUAGES: Record<SupportedLanguage, LanguageInfo> = {
  "zh-Hans": {
    code: "zh-Hans",
    nativeName: "中文(简体)",
    englishName: "Chinese (Simplified)",
  },
  en: { code: "en", nativeName: "English", englishName: "English" },
};

export const DEFAULT_LANGUAGE: SupportedLanguage = "zh-Hans";

const STORAGE_KEY = "language-preference";

export function isSupportedLanguage(code: string): code is SupportedLanguage {
  return (SUPPORTED_LANGUAGE_CODES as readonly string[]).includes(code);
}

function mapDeviceLocale(): SupportedLanguage {
  try {
    const locales = getLocales();
    for (const locale of locales) {
      if (!locale.languageCode) continue;
      // 所有中文变体都映射到 zh-Hans(v0.1 暂不支持 zh-Hant)
      if (locale.languageCode === "zh") return "zh-Hans";
      if (isSupportedLanguage(locale.languageCode)) return locale.languageCode;
    }
  } catch {
    // expo-localization unavailable
  }
  return DEFAULT_LANGUAGE;
}

export async function getStoredLanguagePreference(): Promise<SupportedLanguage | null> {
  try {
    const saved = await AsyncStorage.getItem(STORAGE_KEY);
    if (saved && isSupportedLanguage(saved)) return saved;
  } catch {
    // ignore
  }
  return null;
}

export async function detectLanguage(): Promise<SupportedLanguage> {
  const saved = await getStoredLanguagePreference();
  if (saved !== null) return saved;
  return mapDeviceLocale();
}

/** Save user's explicit choice. Pass `"system"` to follow device locale. */
export async function saveLanguagePreference(
  lang: SupportedLanguage | "system",
): Promise<void> {
  if (lang === "system") {
    await AsyncStorage.removeItem(STORAGE_KEY);
  } else {
    await AsyncStorage.setItem(STORAGE_KEY, lang);
  }
}
