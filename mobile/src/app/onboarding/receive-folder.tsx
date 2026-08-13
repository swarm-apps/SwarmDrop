import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { FolderOpen, Search, Share2 } from "lucide-react-native";
import { useState } from "react";
import { View } from "react-native";
import {
  OnboardingButton,
  OnboardingDots,
  OnboardingScreen,
} from "@/components/onboarding/onboarding-scaffold";
import { Text } from "@/components/ui/text";
import { nextRouteAfter } from "@/core/onboarding-flow";
import {
  pickReceiveLocation,
  useReceiveLocation,
} from "@/core/receive-location";
import { useThemeColors } from "@/hooks/useThemeColors";
import { lastPathSegment } from "@/lib/utils";

/**
 * 接收目录选择（**仅 Android**）。
 *
 * 为什么必须问：Android 11 起应用私有目录连系统文件管理器都进不去，把它当默认落点等于
 * 把收到的文件锁进一座孤岛——用户既在文件管理器里找不到它们，也没法在发送页把它们选出来
 * 转发给第三台设备（发送侧四个来源全是系统 picker，一个都看不见私有目录）。
 * iOS 不走这一步：`Documents` 经文件共享本就对用户可见，无需也无从选择。
 *
 * 这一步**不可跳过**。没有落点就无法接收，与其让用户在收到第一个传输请求时才撞上，
 * 不如在这里花十秒钟解决。
 */
export default function ReceiveFolder() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const location = useReceiveLocation();

  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const chosen = location.status === "ready" ? location.uri : null;

  const onPick = async () => {
    setPicking(true);
    setError(null);
    // 取消后留在原地、什么都不说——用户点开看看又退出来，不该收到一条红字。
    const picked = await pickReceiveLocation();
    if (picked.outcome === "unusable") {
      setError(t`这个位置暂时不可用，换一个试试。`);
    }
    setPicking(false);
  };

  const onContinue = () => {
    router.push(nextRouteAfter("receive-folder") as never);
  };

  const benefits = [
    { Icon: Search, text: t`在系统文件管理器里直接找到收到的文件` },
    { Icon: Share2, text: t`把收到的文件再发给别的设备` },
  ];

  return (
    <OnboardingScreen
      footer={
        <>
          {chosen ? (
            <OnboardingButton
              label={<Trans>继续</Trans>}
              onPress={onContinue}
              testID="onboarding-receive-folder-continue-button"
            />
          ) : (
            <OnboardingButton
              label={<Trans>选择文件夹</Trans>}
              onPress={onPick}
              loading={picking}
              accessibilityLabel={t`选择文件夹`}
              testID="onboarding-receive-folder-pick-button"
            />
          )}
          <OnboardingDots stepId="receive-folder" />
        </>
      }
    >
      <View className="flex-1 justify-center gap-9">
        <View className="gap-3">
          <Text className="text-[26px] font-bold text-foreground">
            <Trans>把收到的文件存到哪？</Trans>
          </Text>
          <Text className="text-[15px] leading-6 text-muted-foreground">
            <Trans>
              选一个你平时能找到的文件夹，比如「下载」。收到的文件就归你管——用任何应用
              打开、移动、删除都可以。
            </Trans>
          </Text>
        </View>

        <View className="gap-3.5">
          {benefits.map(({ Icon, text }) => (
            <View key={text} className="flex-row items-center gap-3">
              <View className="size-9 items-center justify-center rounded-full bg-primary/10">
                <Icon color={colors.primary} size={17} />
              </View>
              <Text className="flex-1 text-[14px] text-muted-foreground">
                {text}
              </Text>
            </View>
          ))}
        </View>

        {chosen ? (
          <View className="flex-row items-center gap-3 rounded-xl bg-primary/10 px-3.5 py-3">
            <FolderOpen color={colors.primary} size={17} />
            <View className="min-w-0 flex-1">
              <Text className="text-[12px] text-muted-foreground">
                <Trans>已选择</Trans>
              </Text>
              <Text
                className="text-[14px] font-medium text-foreground"
                numberOfLines={1}
                testID="onboarding-receive-folder-chosen"
              >
                {lastPathSegment(chosen)}
              </Text>
            </View>
            <Text
              className="text-[13px] font-semibold text-primary-ink"
              onPress={onPick}
              accessibilityRole="button"
            >
              <Trans>更换</Trans>
            </Text>
          </View>
        ) : null}

        {error ? (
          <Text className="text-[13px] text-destructive">{error}</Text>
        ) : null}
      </View>
    </OnboardingScreen>
  );
}
