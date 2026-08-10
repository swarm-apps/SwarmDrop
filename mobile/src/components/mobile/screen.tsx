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
import { SafeAreaView } from "react-native-safe-area-context";
import { Text } from "@/components/ui/text";
import { useBottomSafePadding } from "@/hooks/useBottomSafePadding";
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

/**
 * 同上,但用于**下方挂着常驻导航条**的列表(`AppScreen` 的 `header` 槽)。
 * 顶部 16 是导航条与内容之间的标准呼吸位;内容仍能滚到导航条下沿,不留固定空白带。
 * 各页别再自己算「补回多少」——那正是 activity / 两个搜索页此前各写各的来源。
 */
export const LIST_CONTENT_PADDING_UNDER_HEADER = {
  ...LIST_CONTENT_PADDING,
  paddingTop: 16,
} as const;

/**
 * 卡片列表的行间距(`ItemSeparatorComponent`)。模块级组件,引用稳定。
 * 住在这里而不是某个卡片模块旁边 —— 它没有任何业务语义,谁都能用。
 */
export function ListItemGap() {
  return <View className="h-2" />;
}

interface AppScreenProps {
  children: ReactNode;
  scroll?: boolean;
  testID?: string;
  className?: string;
  contentClassName?: string;
  /**
   * 常驻顶部导航条(`SettingsHeader` / `SearchHeader`),渲染在滚动区**之外**。
   *
   * 导航条带返回入口和右侧操作,跟着内容滚走后一屏之外就够不着了,只剩系统手势;
   * 搜索页更糟——改个关键词得先翻回顶部。放这个槽里由本组件保证它常驻。
   * 两个 header 组件都自带水平内边距,所以这里不加任何 padding,直接整幅渲染。
   *
   * 头与内容之间的间距由 `contentClassName` 表达(列表页则由列表自己的
   * `contentContainerStyle` 表达),本组件不替调用方猜。
   *
   * 反例是 tab 根页的 `AppHeader`(大标题、无返回入口)——那是 iOS large title 语义,
   * 应当跟着内容滚,**不要**放进这个槽。
   */
  header?: ReactNode;
  /**
   * 内容盒不带内边距 —— 给**自带 `contentContainerStyle` 的虚拟化列表**用
   * (`FlatList` / `SectionList`,内边距走 `LIST_CONTENT_PADDING*`)。
   *
   * 有它之前每个列表页都得写 `contentClassName="px-0 pb-0"` 去抵消本组件的默认
   * `px-5 pb-8` —— 一个纯为了取消默认值而存在的魔法串,**漏写 `pb-0` 会静默多出
   * 32+32=64px 的底部死区,没有任何东西会拦**。布尔量表达意图,也不会漏一半。
   */
  bare?: boolean;
  /**
   * 常驻底部停靠区,渲染在滚动内容之外。
   *
   * **只给 tab 屏的 HomeDock 这类「要避开 iOS 26 浮动 tab 胶囊」的东西用。**
   * `BottomActionBar`(以及包着它的 `device/groups` 新建栏)已自己经
   * `useBottomSafePadding()` 吃掉 bottom inset —— 它们进这个槽会让下面的
   * SafeAreaView 再垫一次,底部空两遍。
   * 那类底栏直接放 `children` 里(见 `inbox/[itemId].tsx`)。
   */
  footer?: ReactNode;
}

export function AppScreen({
  children,
  scroll,
  testID,
  className,
  contentClassName,
  header,
  bare,
  footer,
}: AppScreenProps) {
  const contentPadding = bare ? "" : "px-5 pb-8";
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
      {header}
      {scroll ? (
        <ScrollView
          showsVerticalScrollIndicator={false}
          // RN 默认的 "never" 会把键盘竖着时的第一次点击吞掉用来收键盘(搜索页 autoFocus,
          // 于是要点两下才打得开结果行)。没做成 prop:零调用方,且它只能作用于这条 scroll
          // 分支 —— 列表页走 `bare`,传了也会被静默忽略,那种 prop 比写死更坏。
          // 列表页要覆盖就设在自己的 FlatList/SectionList 上(transfer/search.tsx 即如此)。
          keyboardShouldPersistTaps="handled"
          contentContainerClassName={cn(
            "flex-grow",
            contentPadding,
            contentClassName,
          )}
        >
          {children}
        </ScrollView>
      ) : (
        <View className={cn("flex-1", contentPadding, contentClassName)}>
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
  disabled?: boolean;
  testID?: string;
}

export function HeaderIconButton({
  icon: Icon,
  label,
  onPress,
  disabled,
  testID,
}: IconButtonProps) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityLabel={label}
      accessibilityRole="button"
      testID={testID}
      // active/disabled 两个反馈值是 DESIGN.md 钦定的(手写按钮统一 70/50),别另取。
      className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70 disabled:opacity-50"
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

/**
 * **本文件里唯一的底栏组件** —— stack / 详情屏的固定底部动作栏:
 * 横排动作 + `border-t` + 不透明 `bg-background` + 安全区 padding,渲染在滚动内容之外。
 *
 * 这里曾经还有一个 `BottomActionArea`(无背景、不吃安全区)。它只有一个合法调用点
 * (tab 屏的 HomeDock,下面压着不透明 NativeTabs),却被设备详情页误用,
 * 直接造成「取消配对 / 阻止设备 / 改接收策略」整块功能够不到。**已删除并内联进 HomeDock**,
 * 于是「选错底栏组件」这件事在类型层面就不再可能发生。判据见 DESIGN.md 的
 * `Bottom Action Contract (mobile)`。
 *
 * ⚠️ 放 `AppScreen` 的 **children** 里,不要进 `footer` 槽 —— 那个槽下面的 SafeAreaView
 * 在 iOS 会再垫一次 bottom inset。
 */
export function BottomActionBar({
  children,
  testID,
}: {
  children: ReactNode;
  testID?: string;
}) {
  // 底距 = 系统占用 + 呼吸位(相加,不是取大),与 pt-3 对称。理由见 useBottomSafePadding。
  const paddingBottom = useBottomSafePadding();
  return (
    <View
      className="flex-row items-center gap-3 border-t border-border bg-background px-5 pt-3"
      style={{ paddingBottom }}
      testID={testID}
    >
      {children}
    </View>
  );
}

/**
 * 分段控件——一组互斥选项的切换条（附近设备筛选、邀请模式切换）。
 *
 * 语义由 `variant` 决定：`tabs` 切换的是同一件事的两个视图，`filter` 只是过滤同一份
 * 列表。视觉令牌与触控高度两者共用一份——它俩会在同一个 sheet 里前后脚出现，
 * 各写一份必然漂移（曾经一个 py-1.5、一个 min-h-9，无障碍角色也不一样）。
 */
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  variant = "filter",
  testID,
}: {
  value: T;
  options: Array<{
    value: T;
    label: ReactNode;
    icon?: LucideIcon;
    testID?: string;
  }>;
  onChange: (value: T) => void;
  variant?: "tabs" | "filter";
  testID?: string;
}) {
  const colors = useThemeColors();
  const isTabs = variant === "tabs";
  return (
    <View
      className="flex-row gap-1 rounded-lg bg-muted p-0.5"
      accessibilityRole={isTabs ? "tablist" : undefined}
      testID={testID}
    >
      {options.map((option) => {
        const active = option.value === value;
        const Icon = option.icon;
        return (
          <Pressable
            key={option.value}
            onPress={() => onChange(option.value)}
            accessibilityRole={isTabs ? "tab" : "button"}
            accessibilityState={{ selected: active }}
            testID={option.testID}
            className={cn(
              "min-h-9 flex-1 flex-row items-center justify-center gap-1.5 rounded-md px-2 active:opacity-70",
              active && "bg-card",
            )}
          >
            {Icon ? (
              <Icon
                color={active ? colors.foreground : colors.mutedForeground}
                size={14}
              />
            ) : null}
            <Text
              className={cn(
                "text-[12px] font-medium",
                active ? "text-foreground" : "text-muted-foreground",
              )}
            >
              {option.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}
