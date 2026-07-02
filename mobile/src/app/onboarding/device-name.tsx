import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { ArrowLeft, Smartphone } from "lucide-react-native";
import { useEffect, useState } from "react";
import { Pressable, TextInput, View } from "react-native";
import {
  OnboardingButton,
  OnboardingDots,
  OnboardingScreen,
} from "@/components/onboarding/onboarding-scaffold";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { applyDeviceName, suggestedDeviceName } from "@/lib/device-name";
import { useOnboardingStore } from "@/stores/onboarding-store";
import { usePreferencesStore } from "@/stores/preferences-store";

export default function DeviceName() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const nextStep = useOnboardingStore((s) => s.nextStep);
  const existing = usePreferencesStore((s) => s.deviceName);

  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 首次进入:用 expo-device 给一个合理的默认值(之前存过名字就用旧的)
  useEffect(() => {
    setName(existing?.trim() || suggestedDeviceName());
  }, [existing]);

  const trimmed = name.trim();
  const disabled = saving || trimmed.length === 0;

  const onNext = async () => {
    setSaving(true);
    setError(null);
    try {
      await applyDeviceName(trimmed);
      nextStep();
      router.push("/onboarding/setup" as never);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <OnboardingScreen
      footer={
        <>
          <OnboardingButton
            label={<Trans>继续</Trans>}
            onPress={onNext}
            disabled={disabled}
            loading={saving}
            accessibilityLabel={t`继续`}
            testID="onboarding-device-name-continue-button"
          />
          <OnboardingDots step={1} />
        </>
      }
    >
      <View className="gap-6">
        <Pressable
          onPress={() => router.back()}
          hitSlop={12}
          accessibilityRole="button"
          accessibilityLabel={t`返回`}
          className="-ml-2 size-11 items-center justify-center self-start active:opacity-70"
        >
          <ArrowLeft color={colors.foreground} size={24} />
        </Pressable>

        <View className="size-24 items-center justify-center self-center rounded-full bg-primary/10">
          <Smartphone color={colors.primary} size={48} strokeWidth={1.5} />
        </View>

        <View className="items-center gap-2.5">
          <Text className="text-center text-[22px] font-bold text-foreground">
            <Trans>给设备取个名字</Trans>
          </Text>
          <Text className="max-w-[300px] text-center text-[15px] leading-[22px] text-muted-foreground">
            <Trans>其他设备配对时会看到这个名称,可随时在设置里修改。</Trans>
          </Text>
        </View>

        <View className="mt-3 gap-2">
          <Text className="text-[14px] font-semibold text-foreground">
            <Trans>设备名称</Trans>
          </Text>
          <TextInput
            value={name}
            onChangeText={setName}
            autoFocus
            maxLength={40}
            accessibilityLabel={t`设备名称`}
            placeholder={t`我的 iPhone`}
            placeholderTextColor={colors.mutedForeground}
            className="h-12 rounded-xl border border-border bg-card px-3.5 text-[16px] text-foreground"
            testID="onboarding-device-name-input"
          />
          {error !== null ? (
            <Text className="text-[13px] text-destructive-ink">{error}</Text>
          ) : null}
        </View>
      </View>
    </OnboardingScreen>
  );
}
