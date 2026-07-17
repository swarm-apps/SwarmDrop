import AsyncStorage from "@react-native-async-storage/async-storage";
import { Appearance } from "react-native";

export type ThemePreference = "system" | "light" | "dark";

const THEME_KEY = "theme-preference";

export async function restoreThemePreference(): Promise<void> {
  try {
    const saved = await AsyncStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "dark") {
      Appearance.setColorScheme(saved);
    }
  } catch (_e) {
    // ignore
  }
}

export async function saveThemePreference(
  preference: ThemePreference,
): Promise<void> {
  await AsyncStorage.setItem(THEME_KEY, preference);
  Appearance.setColorScheme(
    preference === "system" ? "unspecified" : preference,
  );
}

export async function getThemePreference(): Promise<ThemePreference> {
  try {
    const saved = await AsyncStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "dark" || saved === "system") {
      return saved;
    }
  } catch (_e) {
    // ignore
  }
  return "system";
}
