import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { Lock, Radio, ShieldCheck } from "lucide-react-native";
import { Image, View } from "react-native";
import {
  OnboardingButton,
  OnboardingDots,
  OnboardingScreen,
} from "@/components/onboarding/onboarding-scaffold";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { useOnboardingStore } from "@/stores/onboarding-store";

export default function Welcome() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const nextStep = useOnboardingStore((s) => s.nextStep);

  // 技术卖点降为次要:主视觉是"能帮我干嘛"的人话价值主张,这些是补充的安心感。
  const features = [
    { Icon: ShieldCheck, text: t`端到端加密,只有你和对方能看到` },
    { Icon: Radio, text: t`跨网络直连,不同 WiFi 也能传` },
    { Icon: Lock, text: t`无需账号,设备身份只存在本机` },
  ];

  const onNext = () => {
    nextStep();
    router.push("/onboarding/device-name" as never);
  };

  return (
    <OnboardingScreen
      footer={
        <>
          <OnboardingButton
            label={<Trans>开始使用</Trans>}
            onPress={onNext}
            testID="onboarding-start-button"
          />
          <OnboardingDots step={0} />
        </>
      }
    >
      <View className="flex-1 justify-center gap-10">
        <View className="items-center gap-4">
          <Image
            source={require("../../../assets/images/icon.png")}
            style={{ width: 84, height: 84, borderRadius: 20 }}
            accessibilityLabel="SwarmDrop"
          />
          <View className="items-center gap-2">
            <Text className="text-center text-[26px] font-bold text-foreground">
              <Trans>在你的设备之间,安全地传文件</Trans>
            </Text>
            <Text className="max-w-[300px] text-center text-[15px] text-muted-foreground">
              <Trans>手机和电脑直接互传 —— 不经过账号,也不上传云端。</Trans>
            </Text>
          </View>
        </View>

        <View className="gap-3.5">
          {features.map(({ Icon, text }) => (
            <View key={text} className="flex-row items-center gap-3">
              <View className="size-9 items-center justify-center rounded-full bg-primary/10">
                <Icon color={colors.primary} size={18} strokeWidth={2} />
              </View>
              <Text className="flex-1 text-[14px] text-foreground">{text}</Text>
            </View>
          ))}
        </View>
      </View>
    </OnboardingScreen>
  );
}
