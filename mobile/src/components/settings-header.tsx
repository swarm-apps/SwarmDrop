import type { ReactNode } from "react";
import { View } from "react-native";
import { HeaderBackButton } from "@/components/header-back-button";
import { Text } from "@/components/ui/text";

interface SettingsHeaderProps {
  title: string;
  right?: ReactNode;
}

/**
 * 二级页导航条:返回 + 标题 + 可选右侧操作。
 *
 * 与 `SearchHeader` 同属「导航条」族,三条契约要一起守:
 * - **自带 `px-5`、整幅渲染**,调用方直接塞进 `AppScreen` 的 `header` 槽,不要再包 padding;
 * - **返回入口走 `HeaderBackButton`**(共享定义),不要在这里另画一个箭头;
 * - **高度 `h-14`(56)**,与 `SearchHeader` 的 `min-h-14` 对齐(两者曾是 52/56,转场时标题跳)。
 *
 * `px-5`(20)不是随手取的:它与 `AppScreen` 的内容盒、`LIST_CONTENT_PADDING` 的
 * `paddingHorizontal` 同值,于是返回按钮的**左边缘与下方卡片列的左边缘落在同一条竖线上**。
 * 导航条按 iOS 惯例本可以用 16,但那是给裸图标的——图标自带视觉留白,差 4px 看不出来;
 * 换成带底色的 44px 方钮后,两条实边错开 4px 一眼就能看见。
 */
export function SettingsHeader({ title, right }: SettingsHeaderProps) {
  return (
    <View className="h-14 flex-row items-center justify-between gap-3 px-5">
      <View className="flex-1 flex-row items-center gap-2">
        <HeaderBackButton />
        <Text
          className="min-w-0 flex-1 text-[16px] font-semibold text-foreground"
          numberOfLines={1}
        >
          {title}
        </Text>
      </View>
      {right}
    </View>
  );
}
