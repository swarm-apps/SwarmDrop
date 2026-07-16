import type { LucideIcon } from "lucide-react-native";
import type { ReactNode } from "react";
import { Pressable, View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";

interface SettingRowProps {
  icon?: LucideIcon;
  label: string;
  description?: string;
  children?: ReactNode;
  onPress?: () => void;
}

export function SettingRow({
  icon: Icon,
  label,
  description,
  children,
  onPress,
}: SettingRowProps) {
  const colors = useThemeColors();
  const Root = onPress !== undefined ? Pressable : View;

  return (
    <Root
      onPress={onPress}
      accessibilityRole={onPress !== undefined ? "button" : undefined}
      accessibilityLabel={label}
      className="flex-row items-center justify-between px-3.5 py-3 gap-3"
    >
      <View className="flex-row items-center gap-3 flex-1">
        {Icon ? (
          <View className="h-8 w-8 items-center justify-center rounded-lg bg-muted">
            <Icon color={colors.mutedForeground} size={16} />
          </View>
        ) : null}
        <View className="flex-1">
          <Text className="text-[14px] text-foreground">{label}</Text>
          {description ? (
            <Text className="text-[12px] leading-4 text-muted-foreground mt-0.5">
              {description}
            </Text>
          ) : null}
        </View>
      </View>
      {children}
    </Root>
  );
}

export function SettingSection({
  label,
  children,
}: {
  label?: string;
  children: ReactNode;
}) {
  return (
    <View className="gap-2">
      {label ? (
        <Text className="px-1 text-[12px] font-semibold uppercase tracking-wider text-muted-foreground">
          {label}
        </Text>
      ) : null}
      <View className="rounded-lg border border-border bg-card overflow-hidden">
        {children}
      </View>
    </View>
  );
}

export function SettingDivider() {
  return <View className="h-px bg-border" />;
}
