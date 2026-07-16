import type { ReactNode } from "react";
import { Pressable, ScrollView } from "react-native";
import { Text } from "@/components/ui/text";
import { cn } from "@/lib/utils";

/**
 * 筛选 rail 容器:横滑 + full-bleed 出血。`-mx-5`/`px-5` 与屏幕水平 padding
 * (AppScreen `px-5` / LIST_CONTENT_PADDING 的 20)耦合 —— 集中在这里,
 * 屏幕 padding 调整时只改一处。
 */
export function FilterChipRail({
  children,
  testID,
}: {
  children: ReactNode;
  testID?: string;
}) {
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      className="-mx-5 h-10 max-h-10"
      contentContainerClassName="gap-2 px-5"
      testID={testID}
    >
      {children}
    </ScrollView>
  );
}

/**
 * 筛选 chip —— 收件箱/传输记录的筛选 rail 共用原语:label + 计数,单选高亮。
 */
export function FilterChip({
  active,
  label,
  count,
  onPress,
  testID,
}: {
  active: boolean;
  label: ReactNode;
  count: number;
  onPress: () => void;
  testID?: string;
}) {
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityState={{ selected: active }}
      testID={testID}
      className={cn(
        "h-9 min-w-16 items-center justify-center rounded-xl border px-3 active:opacity-70",
        active ? "border-primary bg-primary" : "border-border bg-card",
      )}
    >
      <Text
        className={cn(
          "text-[13px] font-semibold",
          active ? "text-primary-foreground" : "text-foreground",
        )}
        numberOfLines={1}
      >
        {label} {count}
      </Text>
    </Pressable>
  );
}
