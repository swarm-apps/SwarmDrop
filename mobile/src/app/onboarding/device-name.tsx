import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { Smartphone } from "lucide-react-native";
import { useEffect, useState } from "react";
import { TextInput, View } from "react-native";
import { HeaderBackButton } from "@/components/header-back-button";
import {
  OnboardingButton,
  OnboardingDots,
  OnboardingScreen,
} from "@/components/onboarding/onboarding-scaffold";
import { Text } from "@/components/ui/text";
import { nextRouteAfter } from "@/core/onboarding-flow";
import { useThemeColors } from "@/hooks/useThemeColors";
import {
  applyDeviceName,
  DEVICE_NAME_MAX_CHARS,
  suggestedDeviceName,
} from "@/lib/device-name";
import { getErrorMessage } from "@/lib/errors";
import { usePreferencesStore } from "@/stores/preferences-store";

export default function DeviceName() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const existing = usePreferencesStore((s) => s.deviceName);

  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 首次进入:用 expo-device 给一个合理的默认值(之前存过名字就用旧的)
  useEffect(() => {
    setName(existing?.trim() || suggestedDeviceName(t`我的设备`));
  }, [existing, t]);

  const trimmed = name.trim();
  const disabled = saving || trimmed.length === 0;

  const onNext = async () => {
    setSaving(true);
    setError(null);
    try {
      // onboarding 阶段节点还没起来,core 的改名编排走「只落盘」分支并正常返回;
      // 名字会在随后 startNode 时进本机 OsInfo。存不住会 throw,走下面的 catch。
      await applyDeviceName(trimmed);
      router.push(nextRouteAfter("device-name") as never);
    } catch (err) {
      setError(getErrorMessage(err));
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
          <OnboardingDots stepId="device-name" />
        </>
      }
    >
      <View className="gap-6">
        {/* 引导流没有导航条,但返回入口仍用同一个组件(自带 44px 与 ChevronLeft);
            `self-start` 让它不被父级的 gap-6 拉伸。 */}
        <View className="self-start">
          <HeaderBackButton />
        </View>

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
            maxLength={DEVICE_NAME_MAX_CHARS}
            accessibilityLabel={t`设备名称`}
            placeholder={t`我的 iPhone`}
            placeholderTextColor={colors.mutedForeground}
            className="rounded-xl border border-border bg-card px-3.5 py-3.5 text-[16px] text-foreground"
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

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
