/**
 * AppBottomSheet —— 全 App 统一的 bottom sheet 基线封装。
 *
 * 统一了:标准 backdrop(点击关闭 + 0.4 遮罩)、card 背景、border handle、下拉关闭,
 * 以及**内容自适应高度**(`enableDynamicSizing`,v5 默认行为)——sheet 高度按内容测量,
 * 内容少时不再留大片空白。取代此前部分 sheet 写死的 `snapPoints={["62%","88%"]}`
 * 固定百分比(`enableDynamicSizing={false}`)导致的底部留白 + 各 sheet 高度策略不一致。
 *
 * 三种内容容器:
 * - 默认(`scrollable` 省略):`BottomSheetView`,适合短内容,整体随内容高度收缩。
 * - `scrollable`:`BottomSheetScrollView` + `maxDynamicContentSize` 封顶(默认屏高 90%),
 *   内容超过上限时在 sheet 内滚动,适合长表单 / 带 footer 的编辑器。
 * - `virtualized`:children 自己提供 `BottomSheetFlatList` 等虚拟列表；封装只提供固定高度容器。
 *
 * 参考基线:`InboxActionsSheet`(inbox/[itemId].tsx)。
 */

import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  type BottomSheetFooterProps,
  BottomSheetModal,
  type BottomSheetModalProps,
  BottomSheetScrollView,
  BottomSheetView,
} from "@gorhom/bottom-sheet";
import { forwardRef, type ReactNode, useCallback } from "react";
import {
  type StyleProp,
  useWindowDimensions,
  View,
  type ViewStyle,
} from "react-native";
import { useThemeColors } from "@/hooks/useThemeColors";

/** 与 BottomSheetModal 的命令式方法(present / dismiss)一致。 */
export type AppBottomSheetRef = BottomSheetModal;

interface AppBottomSheetProps {
  children: ReactNode;
  /** true → 用可滚动容器 + maxDynamicContentSize 封顶(长内容 / 带 footer)。 */
  scrollable?: boolean;
  /** true → children 自己提供 BottomSheetFlatList 等虚拟列表。 */
  virtualized?: boolean;
  /** 可滚动时的最大高度占屏比,默认 0.9。 */
  maxHeightRatio?: number;
  /** 内容容器 padding 等样式(尤其带 footer 时给底部留位)。 */
  contentContainerStyle?: StyleProp<ViewStyle>;
  /** 内容容器 testID(迁移时保留原 sheet 的 testID,如 device-policy-sheet)。 */
  contentTestID?: string;
  footerComponent?: React.FC<BottomSheetFooterProps>;
  /**
   * 带输入框的 sheet 传键盘行为;默认不设(短内容无需)。
   * `keyboardBehavior="interactive"` + `keyboardBlurBehavior="restore"` 是带底部输入
   * sheet 的推荐基线。
   *
   * ⚠️ `androidKeyboardInputMode` 勿设 `"adjustResize"`。gorhom 在
   * `android + adjustResize + interactive` 下会把键盘容器高度强制归零并直接 return
   * (BottomSheet.tsx),把 sheet 上移完全交给系统 window resize;而本 App 是
   * Expo(SDK 56)edge-to-edge,`adjustResize` 只把键盘作为 inset、不会 resize sheet 容器,
   * 于是 sheet 纹丝不动、底部输入被键盘盖住。留空即用 gorhom 默认 `adjustPan`,
   * 走库内主动上移逻辑,edge-to-edge 下才正确避让。
   */
  keyboardBehavior?: BottomSheetModalProps["keyboardBehavior"];
  keyboardBlurBehavior?: BottomSheetModalProps["keyboardBlurBehavior"];
  androidKeyboardInputMode?: BottomSheetModalProps["android_keyboardInputMode"];
  snapPoints?: BottomSheetModalProps["snapPoints"];
  enablePanDownToClose?: boolean;
  onDismiss?: () => void;
}

export const AppBottomSheet = forwardRef<
  AppBottomSheetRef,
  AppBottomSheetProps
>(function AppBottomSheet(
  {
    children,
    scrollable = false,
    virtualized = false,
    maxHeightRatio = 0.9,
    contentContainerStyle,
    contentTestID,
    footerComponent,
    keyboardBehavior,
    keyboardBlurBehavior,
    androidKeyboardInputMode,
    snapPoints,
    enablePanDownToClose = true,
    onDismiss,
  },
  ref,
) {
  const colors = useThemeColors();
  const { height } = useWindowDimensions();

  const renderBackdrop = useCallback(
    (props: BottomSheetBackdropProps) => (
      <BottomSheetBackdrop
        {...props}
        opacity={0.4}
        appearsOnIndex={0}
        disappearsOnIndex={-1}
        pressBehavior={enablePanDownToClose ? "close" : "none"}
      />
    ),
    [enablePanDownToClose],
  );

  return (
    <BottomSheetModal
      ref={ref}
      enableDynamicSizing={!virtualized}
      snapPoints={virtualized ? (snapPoints ?? ["90%"]) : snapPoints}
      // 仅在可滚动时封顶:短内容(BottomSheetView)本就随内容收缩,无需上限也不会裁切。
      maxDynamicContentSize={scrollable ? height * maxHeightRatio : undefined}
      enablePanDownToClose={enablePanDownToClose}
      onDismiss={onDismiss}
      keyboardBehavior={keyboardBehavior}
      keyboardBlurBehavior={keyboardBlurBehavior}
      android_keyboardInputMode={androidKeyboardInputMode}
      backdropComponent={renderBackdrop}
      footerComponent={footerComponent}
      backgroundStyle={{ backgroundColor: colors.card }}
      handleIndicatorStyle={{ backgroundColor: colors.border }}
    >
      {virtualized ? (
        <View
          testID={contentTestID}
          style={[{ flex: 1 }, contentContainerStyle]}
        >
          {children}
        </View>
      ) : scrollable ? (
        <BottomSheetScrollView
          testID={contentTestID}
          contentContainerStyle={contentContainerStyle}
        >
          {children}
        </BottomSheetScrollView>
      ) : (
        <BottomSheetView testID={contentTestID} style={contentContainerStyle}>
          {children}
        </BottomSheetView>
      )}
    </BottomSheetModal>
  );
});
