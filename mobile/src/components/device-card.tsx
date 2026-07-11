import { Trans, useLingui } from "@lingui/react/macro";
import { SendHorizontal } from "lucide-react-native";
import { memo } from "react";
import { Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { ConnectionBadge } from "@/components/connection-badge";
import { TrustBadge } from "@/components/trust-badge";
import { Text } from "@/components/ui/text";
import { canSendToDevice, resolveTrustLevel } from "@/core/device-trust";
import { useThemeColors } from "@/hooks/useThemeColors";
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { cn } from "@/lib/utils";

interface DeviceCardProps {
  device: DeviceInfo;
  /** 本机显示名（别名优先）；省略时回退到对端名称 / hostname。 */
  displayName?: string;
  /** 该设备所属分组名，用作同名消歧的次级身份信息。 */
  groupNames?: string[];
  /** `hostname · 短 PeerId` 次级身份提示。 */
  identityHint?: string;
  /** 当前列表内存在同名设备时为 true，触发展示次级身份信息。 */
  showIdentityHint?: boolean;
  onPress?: (device: DeviceInfo) => void;
  onSend?: (device: DeviceInfo) => void;
  variant?: "card" | "row";
  testID?: string;
}

/**
 * 主屏设备入口。row 用于手机首页列表,card 保留给更宽的分组布局。
 */
function DeviceCardComponent({
  device,
  displayName: displayNameProp,
  groupNames = [],
  identityHint,
  showIdentityHint = false,
  onPress,
  onSend,
  variant = "card",
  testID,
}: DeviceCardProps) {
  const colors = useThemeColors();
  const { t } = useLingui();
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const isOnline = device.status === "online";
  const displayName = displayNameProp ?? deviceDisplayName(device);
  const trustLevel = resolveTrustLevel(device);
  const sendable = canSendToDevice(device);
  // 同名时把分组 + `hostname · 短 PeerId` 合成一行次级身份信息。
  const identityLine =
    showIdentityHint || groupNames.length > 0
      ? [...groupNames, showIdentityHint ? identityHint : undefined]
          .filter(Boolean)
          .join(" · ")
      : null;

  if (variant === "row") {
    return (
      <Pressable
        onPress={() => onPress?.(device)}
        accessibilityRole="button"
        accessibilityLabel={displayName}
        testID={testID}
        className={cn(
          "min-h-[76px] flex-row items-center gap-3 rounded-lg border border-border bg-card p-3.5",
          "active:opacity-70",
          !isOnline && "opacity-65",
        )}
      >
        <View className="size-11 items-center justify-center rounded-full bg-muted">
          <Icon color={colors.foreground} size={20} />
        </View>

        <View className="min-w-0 flex-1 gap-1">
          <View className="min-w-0 flex-row items-center gap-2">
            <Text
              className="min-w-0 flex-1 text-[15px] font-semibold text-foreground"
              numberOfLines={1}
            >
              {displayName}
            </Text>
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
                  isOnline ? "text-success-ink" : "text-muted-foreground",
                )}
              >
                {isOnline ? <Trans>在线</Trans> : <Trans>离线</Trans>}
              </Text>
            </View>
          </View>
          <View className="flex-row items-center gap-2">
            <Text
              className="min-w-0 flex-1 text-[11px] text-muted-foreground"
              numberOfLines={1}
            >
              {device.os} · {device.platform}
            </Text>
            {isOnline ? (
              <ConnectionBadge
                connection={device.connection}
                latencyMs={device.latencyMs}
                compact
              />
            ) : null}
            <TrustBadge
              level={trustLevel}
              compact
              confirmed={device.trustConfirmed}
            />
          </View>
          {identityLine ? (
            <Text
              className="text-[10px] text-muted-foreground"
              numberOfLines={1}
            >
              {identityLine}
            </Text>
          ) : null}
        </View>

        <Pressable
          onPress={(event) => {
            event.stopPropagation();
            onSend?.(device);
          }}
          disabled={!sendable}
          accessibilityRole="button"
          accessibilityLabel={t`发送文件给 ${displayName}`}
          className="size-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
        >
          <SendHorizontal
            color={sendable ? colors.primaryForeground : colors.mutedForeground}
            size={17}
          />
        </Pressable>
      </Pressable>
    );
  }

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
              isOnline ? "text-success-ink" : "text-muted-foreground",
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
        {identityLine ? (
          <Text className="text-[10px] text-muted-foreground" numberOfLines={1}>
            {identityLine}
          </Text>
        ) : null}
      </View>

      <View className="flex-row items-center justify-between gap-2">
        <View className="min-w-0 flex-1 flex-row flex-wrap items-center gap-1.5">
          <TrustBadge
            level={trustLevel}
            compact
            confirmed={device.trustConfirmed}
          />
          {isOnline ? (
            <ConnectionBadge
              connection={device.connection}
              latencyMs={device.latencyMs}
              compact
            />
          ) : null}
        </View>
        <Pressable
          onPress={(event) => {
            event.stopPropagation();
            onSend?.(device);
          }}
          disabled={!sendable}
          accessibilityRole="button"
          accessibilityLabel={t`发送文件给 ${displayName}`}
          className="size-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
        >
          <SendHorizontal
            color={sendable ? colors.primaryForeground : colors.mutedForeground}
            size={17}
          />
        </Pressable>
      </View>
    </Pressable>
  );
}

/** 主屏设备列表项:memo 避免父屏(含高频 progressBySession 订阅)重渲染时全表重算。 */
export const DeviceCard = memo(DeviceCardComponent);
