import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { CheckCircle2, KeyRound } from "lucide-react-native";
import { useEffect } from "react";
import { ActivityIndicator, View } from "react-native";
import {
  OnboardingButton,
  OnboardingDots,
  OnboardingScreen,
} from "@/components/onboarding/onboarding-scaffold";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { truncateMiddle } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { useOnboardingStore } from "@/stores/onboarding-store";

export default function Setup() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const markCompleted = useOnboardingStore((s) => s.markCompleted);
  const { loadIdentity, peerId, error } = useMobileCoreStore();

  useEffect(() => {
    void loadIdentity();
  }, [loadIdentity]);

  const ready = peerId !== null && error === null;
  const failed = error !== null;

  const onEnter = () => {
    markCompleted();
    router.replace("/(main)" as never);
  };

  return (
    <OnboardingScreen
      footer={
        <>
          {failed ? (
            <OnboardingButton
              label={<Trans>重试</Trans>}
              onPress={() => loadIdentity()}
              testID="onboarding-retry-button"
            />
          ) : (
            <OnboardingButton
              label={<Trans>进入 SwarmDrop</Trans>}
              onPress={onEnter}
              disabled={!ready}
              testID="onboarding-enter-button"
            />
          )}
          <OnboardingDots step={2} />
        </>
      }
    >
      <View className="flex-1 items-center justify-center gap-4">
        <View className="size-24 items-center justify-center rounded-full bg-primary/10">
          {ready ? (
            <CheckCircle2 color={colors.success} size={64} strokeWidth={2} />
          ) : (
            <KeyRound color={colors.primary} size={56} strokeWidth={2} />
          )}
        </View>

        <Text className="text-center text-[22px] font-bold text-foreground">
          {ready ? t`一切就绪` : failed ? t`初始化失败` : t`正在准备你的设备`}
        </Text>
        <Text className="max-w-[300px] text-center text-[15px] leading-[22px] text-muted-foreground">
          {ready
            ? t`设备身份已就绪,可以开始配对和传输文件了`
            : failed
              ? t`请稍后重试,或检查存储权限`
              : t`正在生成本地身份...`}
        </Text>

        {!ready && !failed ? (
          <ActivityIndicator
            color={colors.primary}
            size="large"
            className="my-4"
          />
        ) : null}

        {peerId !== null ? (
          <View className="mt-3 w-full rounded-lg border border-border bg-card p-3.5">
            <Text className="mb-1 text-[12px] text-muted-foreground">
              <Trans>设备 ID</Trans>
            </Text>
            <Text
              className="font-mono text-[13px] text-foreground"
              numberOfLines={1}
            >
              {truncateMiddle(peerId, 16, 6)}
            </Text>
          </View>
        ) : null}

        {error !== null ? (
          <Text className="text-center text-[13px] text-destructive">
            {error}
          </Text>
        ) : null}
      </View>
    </OnboardingScreen>
  );
}
