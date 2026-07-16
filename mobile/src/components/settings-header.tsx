import { useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { ArrowLeft } from "lucide-react-native";
import type { ReactNode } from "react";
import { Pressable, View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";

interface SettingsHeaderProps {
  title: string;
  right?: ReactNode;
}

export function SettingsHeader({ title, right }: SettingsHeaderProps) {
  const router = useRouter();
  const colors = useThemeColors();
  const { t } = useLingui();

  return (
    <View className="h-13 flex-row items-center justify-between gap-3 px-4">
      <View className="flex-row items-center gap-3 flex-1">
        <Pressable
          onPress={() => router.back()}
          hitSlop={12}
          accessibilityRole="button"
          accessibilityLabel={t`返回`}
        >
          <ArrowLeft color={colors.foreground} size={22} />
        </Pressable>
        <Text className="text-[16px] font-semibold text-foreground">
          {title}
        </Text>
      </View>
      {right}
    </View>
  );
}
