import { useLingui } from "@lingui/react/macro";
import { useFocusEffect } from "expo-router";
import {
  Check,
  type LucideIcon,
  Monitor,
  Moon,
  Sun,
} from "lucide-react-native";
import { Fragment, useCallback, useState } from "react";
import { Pressable, View } from "react-native";
import { AppScreen } from "@/components/mobile/screen";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import {
  getThemePreference,
  saveThemePreference,
  type ThemePreference,
} from "@/lib/theme-persistence";
import { toast } from "@/lib/toast";

interface Option {
  value: ThemePreference;
  label: string;
  icon: LucideIcon;
}

export default function ThemeScreen() {
  const colors = useThemeColors();
  const { t } = useLingui();
  const [theme, setTheme] = useState<ThemePreference>("system");

  useFocusEffect(
    useCallback(() => {
      getThemePreference()
        .then(setTheme)
        .catch(() => {});
    }, []),
  );

  const handleSelect = async (value: ThemePreference) => {
    if (theme === value) return;
    setTheme(value);
    try {
      await saveThemePreference(value);
      toast.success(t`主题已更新`);
    } catch (err) {
      console.warn("[theme] saveThemePreference failed:", err);
      toast.error(t`保存失败`, err);
    }
  };

  const options: Option[] = [
    { value: "system", label: t`跟随系统`, icon: Monitor },
    { value: "light", label: t`浅色`, icon: Sun },
    { value: "dark", label: t`深色`, icon: Moon },
  ];

  return (
    <AppScreen
      scroll
      header={<SettingsHeader title={t`外观`} />}
      contentClassName="gap-5 pt-2"
    >
      <SettingSection label={t`选择外观`}>
        {options.map((opt, idx) => {
          const Icon = opt.icon;
          const selected = theme === opt.value;
          return (
            <Fragment key={opt.value}>
              <Pressable
                onPress={() => handleSelect(opt.value)}
                accessibilityRole="button"
                accessibilityLabel={opt.label}
                accessibilityState={{ selected }}
                className="flex-row items-center gap-3 px-3.5 py-3 active:bg-muted"
              >
                <Icon color={colors.mutedForeground} size={18} />
                <Text className="flex-1 text-[14px] text-foreground">
                  {opt.label}
                </Text>
                {selected ? (
                  <View className="h-6 w-6 items-center justify-center">
                    <Check color={colors.primary} size={18} />
                  </View>
                ) : null}
              </Pressable>
              {idx < options.length - 1 ? <SettingDivider /> : null}
            </Fragment>
          );
        })}
      </SettingSection>
    </AppScreen>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
