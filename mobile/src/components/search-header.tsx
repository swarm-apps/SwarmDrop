import { useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { ChevronLeft, Search, X } from "lucide-react-native";
import type { ReactNode } from "react";
import { Pressable, TextInput, View } from "react-native";
import { HeaderIconButton } from "@/components/mobile/screen";
import { useThemeColors } from "@/hooks/useThemeColors";

/**
 * 搜索页头部:返回 + autoFocus 输入框 + 条件清除按钮。
 * 收件箱/传输记录两个搜索页共用;`trailing` 给「检索中」spinner 之类的附加指示留槽。
 */
export function SearchHeader({
  value,
  onChangeText,
  placeholder,
  inputLabel,
  testIDPrefix,
  trailing,
}: {
  value: string;
  onChangeText: (text: string) => void;
  placeholder: string;
  /** 输入框的 accessibilityLabel(如「搜索收件箱」)。 */
  inputLabel: string;
  /** testID 前缀:生成 `{prefix}-back-button` 与 `{prefix}-input`。 */
  testIDPrefix: string;
  trailing?: ReactNode;
}) {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();

  return (
    <View className="min-h-14 flex-row items-center gap-2">
      <HeaderIconButton
        icon={ChevronLeft}
        label={t`返回`}
        onPress={() => router.back()}
        testID={`${testIDPrefix}-back-button`}
      />
      <View className="min-h-11 min-w-0 flex-1 flex-row items-center gap-2 rounded-xl bg-muted px-3">
        <Search color={colors.mutedForeground} size={16} />
        <TextInput
          autoFocus
          value={value}
          onChangeText={onChangeText}
          accessibilityLabel={inputLabel}
          placeholder={placeholder}
          placeholderTextColor={colors.mutedForeground}
          returnKeyType="search"
          className="min-w-0 flex-1 text-[14px] text-foreground"
          testID={`${testIDPrefix}-input`}
        />
        {trailing}
        {value.length > 0 ? (
          <Pressable
            onPress={() => onChangeText("")}
            accessibilityRole="button"
            accessibilityLabel={t`清除搜索`}
            hitSlop={8}
            className="size-7 items-center justify-center rounded-full bg-card active:opacity-70"
          >
            <X color={colors.mutedForeground} size={14} />
          </Pressable>
        ) : null}
      </View>
    </View>
  );
}
