import {
  DarkTheme,
  DefaultTheme,
  type Theme,
} from "expo-router/react-navigation";
import { useUnstableNativeVariable } from "nativewind";
import { useMemo } from "react";
import { useColorScheme } from "react-native";

function hsl(value: string | undefined): string {
  return value ? `hsl(${value})` : "transparent";
}

/**
 * 从 NativeWind CSS 变量动态读取主题色,global.css 是唯一真相源。
 * 用于需要 JS 颜色值的场景(图标 color prop, React Navigation Theme)。
 */
export function useThemeColors() {
  const background = useUnstableNativeVariable("--background");
  const foreground = useUnstableNativeVariable("--foreground");
  const card = useUnstableNativeVariable("--card");
  const primary = useUnstableNativeVariable("--primary");
  const primaryForeground = useUnstableNativeVariable("--primary-foreground");
  const destructive = useUnstableNativeVariable("--destructive");
  const success = useUnstableNativeVariable("--success");
  const warning = useUnstableNativeVariable("--warning");
  const border = useUnstableNativeVariable("--border");
  const muted = useUnstableNativeVariable("--muted");
  const mutedForeground = useUnstableNativeVariable("--muted-foreground");
  const accent = useUnstableNativeVariable("--accent");
  const accentForeground = useUnstableNativeVariable("--accent-foreground");

  return useMemo(
    () => ({
      background: hsl(background),
      foreground: hsl(foreground),
      card: hsl(card),
      primary: hsl(primary),
      primaryForeground: hsl(primaryForeground),
      destructive: hsl(destructive),
      success: hsl(success),
      warning: hsl(warning),
      border: hsl(border),
      muted: hsl(muted),
      mutedForeground: hsl(mutedForeground),
      accent: hsl(accent),
      accentForeground: hsl(accentForeground),
    }),
    [
      background,
      foreground,
      card,
      primary,
      primaryForeground,
      destructive,
      success,
      warning,
      border,
      muted,
      mutedForeground,
      accent,
      accentForeground,
    ],
  );
}

export function useNavTheme(): Theme {
  const colorScheme = useColorScheme();
  const isDark = colorScheme === "dark";
  const colors = useThemeColors();

  return useMemo(() => {
    const base = isDark ? DarkTheme : DefaultTheme;
    return {
      ...base,
      colors: {
        ...base.colors,
        background: colors.background,
        border: colors.border,
        card: colors.card,
        notification: colors.destructive,
        primary: colors.primary,
        text: colors.foreground,
      },
    };
  }, [isDark, colors]);
}
