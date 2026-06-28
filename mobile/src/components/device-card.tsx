import { Trans, useLingui } from "@lingui/react/macro";
import { SendHorizontal } from "lucide-react-native";
import { Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { TrustBadge } from "@/components/trust-badge";
import { Text } from "@/components/ui/text";
import { canSendToDevice, resolveTrustLevel } from "@/core/device-trust";
import { useThemeColors } from "@/hooks/useThemeColors";
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { cn } from "@/lib/utils";

interface DeviceCardProps {
  device: DeviceInfo;
  onPress?: (device: DeviceInfo) => void;
  onSend?: (device: DeviceInfo) => void;
  testID?: string;
}

/**
 * 主屏 2 列 grid 设备卡片。整张进入详情;发送是独立操作。
 */
export function DeviceCard({
  device,
  onPress,
  onSend,
  testID,
}: DeviceCardProps) {
  const colors = useThemeColors();
  const { t } = useLingui();
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const isOnline = device.status === "online";
  const displayName = deviceDisplayName(device);
  const trustLevel = resolveTrustLevel(device);
  const sendable = canSendToDevice(device);

  return (
    <Pressable
      onPress={() => onPress?.(device)}
      accessibilityRole="button"
      accessibilityLabel={displayName}
      testID={testID}
      className={cn(
        "flex-1 gap-3 rounded-lg border border-border bg-card p-3.5",
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
          {displayName}
        </Text>
        <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
          {device.os} · {device.platform}
        </Text>
      </View>

      <View className="flex-row items-center justify-between gap-2">
        <TrustBadge level={trustLevel} compact />
        <Pressable
          onPress={(event) => {
            event.stopPropagation();
            onSend?.(device);
          }}
          disabled={!sendable}
          accessibilityRole="button"
          accessibilityLabel={t`发送文件`}
          className="size-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
        >
          <SendHorizontal
            color={sendable ? colors.background : colors.mutedForeground}
            size={17}
          />
        </Pressable>
      </View>
    </Pressable>
  );
}
