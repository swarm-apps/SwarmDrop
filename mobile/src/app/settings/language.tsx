import { useLingui } from "@lingui/react/macro";
import { useFocusEffect } from "expo-router";
import { Check } from "lucide-react-native";
import { Fragment, useCallback, useState } from "react";
import { Pressable, View } from "react-native";
import { AppScreen } from "@/components/mobile/screen";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import {
  getStoredLanguagePreference,
  SUPPORTED_LANGUAGE_CODES,
  SUPPORTED_LANGUAGES,
  type SupportedLanguage,
} from "@/i18n/languageDetector";
import { followSystemLanguage, setUserLanguage } from "@/i18n/lingui";
import { toast } from "@/lib/toast";

type Selection = SupportedLanguage | "system";

export default function LanguageScreen() {
  const colors = useThemeColors();
  const { t } = useLingui();
  // null means "follow system"; otherwise the user's explicit pick.
  const [storedLang, setStoredLang] = useState<SupportedLanguage | null>(null);

  useFocusEffect(
    useCallback(() => {
      getStoredLanguagePreference()
        .then(setStoredLang)
        .catch(() => {});
    }, []),
  );

  const handleSelect = async (selection: Selection) => {
    try {
      if (selection === "system") {
        if (storedLang === null) return;
        setStoredLang(null);
        await followSystemLanguage();
      } else {
        if (storedLang === selection) return;
        setStoredLang(selection);
        await setUserLanguage(selection);
      }
      toast.success(t`语言已更新`);
    } catch (err) {
      toast.error(t`保存失败`, err);
    }
  };

  const isSystemSelected = storedLang === null;

  return (
    <AppScreen
      scroll
      header={<SettingsHeader title={t`语言`} />}
      contentClassName="gap-5 pt-2"
    >
      <SettingSection label={t`选择语言`}>
        <Row
          label={t`跟随系统`}
          selected={isSystemSelected}
          onPress={() => handleSelect("system")}
          checkColor={colors.primary}
        />
        <SettingDivider />
        {SUPPORTED_LANGUAGE_CODES.map((code, idx) => (
          <Fragment key={code}>
            <Row
              label={SUPPORTED_LANGUAGES[code].nativeName}
              selected={!isSystemSelected && storedLang === code}
              onPress={() => handleSelect(code)}
              checkColor={colors.primary}
            />
            {idx < SUPPORTED_LANGUAGE_CODES.length - 1 ? (
              <SettingDivider />
            ) : null}
          </Fragment>
        ))}
      </SettingSection>
    </AppScreen>
  );
}

interface RowProps {
  label: string;
  selected: boolean;
  onPress: () => void;
  checkColor: string;
}

function Row({ label, selected, onPress, checkColor }: RowProps) {
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ selected }}
      className="flex-row items-center justify-between px-3.5 py-3 gap-3 active:bg-muted"
    >
      <Text className="flex-1 text-[14px] text-foreground">{label}</Text>
      {selected ? (
        <View className="h-6 w-6 items-center justify-center">
          <Check color={checkColor} size={18} />
        </View>
      ) : null}
    </Pressable>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
