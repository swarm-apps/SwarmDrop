import { Trans } from "@lingui/react/macro";
import { Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { devicePlatformIcon } from "@/lib/device-platform";
import { cn } from "@/lib/utils";

interface DeviceCardProps {
  device: DeviceInfo;
  onPress?: (device: DeviceInfo) => void;
}

/**
 * 主屏 2 列 grid 设备卡片。整张可点击;离线设备置灰但仍可点(详情)。
 */
export function DeviceCard({ device, onPress }: DeviceCardProps) {
  const colors = useThemeColors();
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const isOnline = device.status === "online";

  return (
    <Pressable
      onPress={() => onPress?.(device)}
      accessibilityRole="button"
      accessibilityLabel={device.hostname}
      className={cn(
        "flex-1 gap-3 rounded-xl border border-border bg-card p-3.5",
        "active:opacity-70",
        !isOnline && "opacity-60",
      )}
    >
      <View className="flex-row items-center justify-between">
        <View className="size-9 items-center justify-center rounded-full bg-muted">
          <Icon color={colors.foreground} size={18} />
        </View>
        <View className="flex-row items-center gap-1">
          <View
            className={cn(
              "size-1.5 rounded-full",
              isOnline ? "bg-success" : "bg-muted-foreground",
            )}
          />
          <Text
            className={cn(
              "text-[11px]",
              isOnline ? "text-success" : "text-muted-foreground",
            )}
          >
            {isOnline ? <Trans>在线</Trans> : <Trans>离线</Trans>}
          </Text>
        </View>
      </View>

      <View className="gap-0.5">
        <Text
          className="text-sm font-semibold text-foreground"
          numberOfLines={1}
        >
          {device.hostname}
        </Text>
        <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
          {device.os} · {device.platform}
        </Text>
      </View>
    </Pressable>
  );
}
