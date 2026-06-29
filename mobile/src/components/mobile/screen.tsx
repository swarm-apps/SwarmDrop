import type { LucideIcon } from "lucide-react-native";
import type { ReactNode } from "react";
import { ActivityIndicator, Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

interface AppScreenProps {
  children: ReactNode;
  scroll?: boolean;
  testID?: string;
  className?: string;
  contentClassName?: string;
}

export function AppScreen({
  children,
  scroll,
  testID,
  className,
  contentClassName,
}: AppScreenProps) {
  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className={cn("bg-background", className)}
      edges={["top"]}
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
      className="min-h-14 flex-row items-center justify-between gap-3 px-5 py-3"
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
              className="mt-0.5 text-[12px] text-muted-foreground"
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
        <Text className="text-center text-[12px] text-muted-foreground">
          {description}
        </Text>
      </View>
      {actionLabel != null && onAction != null ? (
        <Pressable
          onPress={onAction}
          disabled={actionDisabled || actionLoading}
          accessibilityRole="button"
          className="min-h-11 min-w-24 items-center justify-center rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-55"
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
