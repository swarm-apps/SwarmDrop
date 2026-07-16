import { Lock } from "lucide-react-native";
import type { ReactNode } from "react";
import { View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

// 加密是恒定事实,不是可变状态 —— 所以这里不穿状态色(绿/蓝)、不做 pill/badge,
// 就是一行 muted 灰墨小字加一枚小锁:在意的人每次都看得到,不在意的人自然滑过。
export function EncryptionNote({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  const colors = useThemeColors();
  return (
    <View className={cn("flex-row items-center gap-1.5", className)}>
      <Lock color={colors.mutedForeground} size={12} />
      <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
        {children}
      </Text>
    </View>
  );
}
