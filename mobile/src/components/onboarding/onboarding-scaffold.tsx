/**
 * 引导流程统一骨架 —— 背景/内边距/安全区/主按钮/进度点全走设计系统 token,
 * 自动适配暗色模式。取代原先三屏各自的 StyleSheet + 硬编码 hex(消重 + token 合规)。
 */

import { useLingui } from "@lingui/react/macro";
import type { ReactNode } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

export function OnboardingScreen({
  children,
  footer,
}: {
  children: ReactNode;
  footer: ReactNode;
}) {
  return (
    // 水平内边距放在内层 View —— safe-area-context 的 SafeAreaView 会用自身 inset padding
    // 覆盖 className 的 paddingHorizontal(左右会变 0、内容贴边),与 AppScreen 同一约定。
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top", "bottom"]}
    >
      <View className="flex-1 px-7 pt-4">{children}</View>
      <View className="gap-4 px-7 pb-4 pt-4">{footer}</View>
    </SafeAreaView>
  );
}

/** 全宽主 CTA。loading 时显示 spinner,用 primaryForeground 与文字同色(过 AA)。 */
export function OnboardingButton({
  label,
  onPress,
  disabled,
  loading,
  accessibilityLabel,
  testID,
}: {
  label: ReactNode;
  onPress: () => void;
  disabled?: boolean;
  loading?: boolean;
  accessibilityLabel?: string;
  testID?: string;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled || loading}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      testID={testID}
      className="min-h-[52px] flex-row items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
    >
      {loading ? (
        <ActivityIndicator color={colors.primaryForeground} />
      ) : (
        <Text className="text-[16px] font-semibold text-primary-foreground">
          {label}
        </Text>
      )}
    </Pressable>
  );
}

/** 进度点(共 3 步),带"第 X 步,共 3 步"无障碍标签。 */
export function OnboardingDots({ step }: { step: 0 | 1 | 2 }) {
  const { t } = useLingui();
  return (
    <View
      className="flex-row items-center justify-center gap-2"
      accessible
      accessibilityLabel={t`第 ${step + 1} 步,共 3 步`}
    >
      {[0, 1, 2].map((i) => (
        <View
          key={i}
          className={cn(
            "rounded-full",
            i === step
              ? "size-2.5 bg-primary"
              : "size-2 bg-muted-foreground/40",
          )}
        />
      ))}
    </View>
  );
}
