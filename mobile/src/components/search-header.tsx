import { useLingui } from "@lingui/react/macro";
import { Search, X } from "lucide-react-native";
import type { ReactNode } from "react";
import { Pressable, TextInput, View } from "react-native";
import { HeaderBackButton } from "@/components/header-back-button";
import { useThemeColors } from "@/hooks/useThemeColors";

/**
 * 搜索页头部:返回 + autoFocus 输入框 + 条件清除按钮。
 * 收件箱/传输记录两个搜索页共用;`trailing` 给「检索中」spinner 之类的附加指示留槽。
 *
 * 与 `SettingsHeader` 同属「导航条」族,三条契约一致:**自带 `px-5`、整幅渲染**
 * (调用方直接塞进 `AppScreen` 的 `header` 槽,不要再包 padding,包了会叠成 40)、
 * **返回走 `HeaderBackButton`**、**高度 56**。
 * `px-5` 与内容盒/列表同值的理由见 `settings-header.tsx` 的注释。
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
  const { t } = useLingui();
  const colors = useThemeColors();

  return (
    <View className="min-h-14 flex-row items-center gap-2 px-5">
      <HeaderBackButton testID={`${testIDPrefix}-back-button`} />
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
