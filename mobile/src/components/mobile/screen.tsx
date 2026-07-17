import type { LucideIcon } from "lucide-react-native";
import type { ReactNode } from "react";
import {
  ActivityIndicator,
  Platform,
  Pressable,
  ScrollView,
  View,
} from "react-native";
import Animated from "react-native-reanimated";
import {
  SafeAreaView,
  useSafeAreaInsets,
} from "react-native-safe-area-context";
import { Text } from "@/components/ui/text";
import { usePulseOpacity } from "@/hooks/usePulseOpacity";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

/**
 * FlatList / SectionList 的内容内边距 —— 与 `AppScreen` 的 `px-5 pb-8`(20/32) + `pt-1`(4)
 * 对齐。列表页从 `AppScreen` 切到虚拟化容器时复用此常量:既避免魔数在多屏漂移,
 * 又给 `contentContainerStyle` 一个稳定引用(每次渲染不新建对象)。
 */
export const LIST_CONTENT_PADDING = {
  paddingHorizontal: 20,
  paddingTop: 4,
  paddingBottom: 32,
} as const;

interface AppScreenProps {
  children: ReactNode;
  scroll?: boolean;
  testID?: string;
  className?: string;
  contentClassName?: string;
  /** 常驻底部停靠区(拇指区),渲染在滚动内容之外,如主屏的 HomeDock。 */
  footer?: ReactNode;
}

export function AppScreen({
  children,
  scroll,
  testID,
  className,
  contentClassName,
  footer,
}: AppScreenProps) {
  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className={cn("bg-background", className)}
      // footer(拇指区 dock)在 iOS 必须避开悬浮 tab bar:原生 SafeAreaView 就地测量自身
      // safeAreaInsets,在 tab 容器内 bottom 会包含 iOS 26 浮动胶囊的高度。Android 的 tab bar
      // 是实体占位、手势条也在 tab bar 之下,但 safe-area-context 仍会把手势条高度上报进
      // bottom inset(不按视图相交计算),加了只会多一截空白 —— 故 bottom edge 仅 iOS 启用。
      // 无 footer 的屏保持只留 top,滚动内容照常延伸到屏幕底。
      edges={footer && Platform.OS === "ios" ? ["top", "bottom"] : ["top"]}
      testID={testID}
    >
      {scroll ? (
        <ScrollView
          showsVerticalScrollIndicator={false}
          contentContainerClassName={cn(
            "flex-grow px-5 pb-8",
            contentClassName,
          )}
        >
          {children}
        </ScrollView>
      ) : (
        <View className={cn("flex-1 px-5 pb-8", contentClassName)}>
          {children}
        </View>
      )}
      {footer}
    </SafeAreaView>
  );
}

interface AppHeaderProps {
  title: ReactNode;
  subtitle?: ReactNode;
  left?: ReactNode;
  right?: ReactNode;
  testID?: string;
}

export function AppHeader({
  title,
  subtitle,
  left,
  right,
  testID,
}: AppHeaderProps) {
  return (
    <View
      className="min-h-14 flex-row items-center justify-between gap-3 py-3"
      testID={testID}
    >
      <View className="min-w-0 flex-1 flex-row items-center gap-3">
        {left}
        <View className="min-w-0 flex-1">
          <Text
            className="text-[18px] font-semibold text-foreground"
            numberOfLines={1}
          >
            {title}
          </Text>
          {subtitle ? (
            <Text
              className="mt-0.5 text-[13px] text-muted-foreground"
              numberOfLines={1}
            >
              {subtitle}
            </Text>
          ) : null}
        </View>
      </View>
      {right}
    </View>
  );
}

interface IconButtonProps {
  icon: LucideIcon;
  label: string;
  onPress: () => void;
  testID?: string;
}

export function HeaderIconButton({
  icon: Icon,
  label,
  onPress,
  testID,
}: IconButtonProps) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      accessibilityLabel={label}
      accessibilityRole="button"
      testID={testID}
      className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70"
    >
      <Icon color={colors.foreground} size={20} />
    </Pressable>
  );
}

export function Surface({
  children,
  className,
  testID,
}: {
  children: ReactNode;
  className?: string;
  testID?: string;
}) {
  return (
    <View
      className={cn("rounded-lg border border-border bg-card p-3.5", className)}
      testID={testID}
    >
      {children}
    </View>
  );
}

interface EmptyStateProps {
  icon: LucideIcon;
  title: ReactNode;
  description: ReactNode;
  actionLabel?: ReactNode;
  onAction?: () => void;
  actionLoading?: boolean;
  actionDisabled?: boolean;
  testID?: string;
  className?: string;
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  actionLoading,
  actionDisabled,
  testID,
  className,
}: EmptyStateProps) {
  const colors = useThemeColors();
  return (
    <View
      className={cn(
        "min-h-44 items-center justify-center gap-4 rounded-lg border border-dashed border-border bg-card px-5 py-10",
        className,
      )}
      testID={testID}
    >
      <View className="size-14 items-center justify-center rounded-full bg-muted">
        <Icon color={colors.mutedForeground} size={26} />
      </View>
      <View className="items-center gap-1">
        <Text className="text-center text-[15px] font-semibold text-foreground">
          {title}
        </Text>
        <Text className="text-center text-[13px] leading-5 text-muted-foreground">
          {description}
        </Text>
      </View>
      {actionLabel != null && onAction != null ? (
        <Pressable
          onPress={onAction}
          disabled={actionDisabled || actionLoading}
          accessibilityRole="button"
          accessibilityState={{
            busy: !!actionLoading,
            disabled: !!(actionDisabled || actionLoading),
          }}
          className="min-h-11 min-w-24 items-center justify-center rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-50"
        >
          {actionLoading ? (
            <ActivityIndicator color={colors.primaryForeground} />
          ) : (
            <Text className="text-[13px] font-semibold text-primary-foreground">
              {actionLabel}
            </Text>
          )}
        </Pressable>
      ) : null}
    </View>
  );
}

interface InlineEmptyStateProps {
  icon: LucideIcon;
  title: ReactNode;
  description?: ReactNode;
  /** 扫描/等待中语义:图标 chip 呼吸脉冲,把「空」表达成「正在进行」。 */
  pulse?: boolean;
  testID?: string;
}

/**
 * 行内空态 —— 比全屏 `EmptyState` 轻一档:用于卡片区块/sheet 分组内的空状态。
 * 同一空态语言(dashed 边框 + muted 圆 chip),但尺寸收紧、无动作按钮。
 */
export function InlineEmptyState({
  icon: Icon,
  title,
  description,
  pulse = false,
  testID,
}: InlineEmptyStateProps) {
  const colors = useThemeColors();
  return (
    <View
      className="items-center gap-2.5 rounded-lg border border-dashed border-border bg-card px-4 py-5"
      testID={testID}
    >
      <IconChipPulse enabled={pulse}>
        <View className="size-9 items-center justify-center rounded-full bg-muted">
          <Icon color={colors.mutedForeground} size={16} />
        </View>
      </IconChipPulse>
      <View className="items-center gap-0.5">
        <Text className="text-center text-[13px] font-medium text-foreground">
          {title}
        </Text>
        {description ? (
          <Text className="text-center text-[12px] leading-4 text-muted-foreground">
            {description}
          </Text>
        ) : null}
      </View>
    </View>
  );
}

function IconChipPulse({
  enabled,
  children,
}: {
  enabled: boolean;
  children: ReactNode;
}) {
  const style = usePulseOpacity({ min: 0.4, duration: 800, enabled });
  return <Animated.View style={style}>{children}</Animated.View>;
}

export function BottomActionArea({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <View className={cn("gap-2 border-t border-border px-5 py-4", className)}>
      {children}
    </View>
  );
}

/**
 * 详情页固定底部动作栏容器:横排动作 + 安全区 padding,渲染在滚动内容之外。
 * 收件箱详情与传输详情共用这层 chrome;按钮形态与状态逻辑保持页内私有。
 */
export function BottomActionBar({
  children,
  testID,
}: {
  children: ReactNode;
  testID?: string;
}) {
  const insets = useSafeAreaInsets();
  return (
    <View
      className="flex-row items-center gap-3 border-t border-border bg-background px-5 pt-3"
      style={{ paddingBottom: Math.max(insets.bottom, 12) }}
      testID={testID}
    >
      {children}
    </View>
  );
}
