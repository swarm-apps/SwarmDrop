import type { ReactNode } from "react";
import { View } from "react-native";
import { Text } from "@/components/ui/text";

/**
 * 通用「左灰标签 + 右对齐值」行 —— 详情卡里的 key-value 原语。
 * 值默认最多 3 行;长 ID/哈希类内容传 mono。
 */
export function KeyValueRow({
  label,
  value,
  mono = false,
  numberOfLines = 3,
}: {
  label: ReactNode;
  value: ReactNode;
  mono?: boolean;
  numberOfLines?: number;
}) {
  return (
    <View className="flex-row items-start justify-between gap-3">
      <Text className="text-[13px] text-muted-foreground">{label}</Text>
      <Text
        className={`flex-1 text-right text-[13px] text-foreground ${mono ? "font-mono" : ""}`}
        numberOfLines={numberOfLines}
      >
        {value}
      </Text>
    </View>
  );
}
