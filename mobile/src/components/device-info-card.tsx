import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Device from "expo-device";
import * as Haptics from "expo-haptics";
import {
  Activity,
  Check,
  Copy,
  Cpu,
  Pencil,
  ShieldCheck,
  Zap,
} from "lucide-react-native";
import { useEffect, useRef, useState } from "react";
import { Platform, Pressable, TextInput, View } from "react-native";
import { useShallow } from "zustand/react/shallow";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { applyDeviceName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import { errorMessage, truncateMiddle } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 设置首页顶部的「设备信息卡」—— 对齐桌面端 DeviceInfoSection。
 * 上半部分:头像(2 字母 initials)+ 设备名(可编辑)+ OS + PeerID
 * 下半部分:已连节点 / 配对设备数 / NAT 状态 三列指标
 */
export function DeviceInfoCard() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const { peerId, runtimeState, networkStatus, pairedDeviceCount } =
    useMobileCoreStore(
      useShallow((s) => ({
        peerId: s.peerId,
        runtimeState: s.runtimeState,
        networkStatus: s.networkStatus,
        pairedDeviceCount: s.devices.filter((d) => d.isPaired).length,
      })),
    );
  const deviceName = usePreferencesStore((s) => s.deviceName);

  const systemHostname = Device.deviceName ?? Device.modelName ?? "SwarmDrop";
  const displayName = deviceName || systemHostname;
  const avatarInitials = displayName.slice(0, 2).toUpperCase();
  const Icon = devicePlatformIcon(`${Device.osName ?? ""}`);
  const osLabel = `${Device.osName ?? ""} ${Device.osVersion ?? ""} · ${Device.modelName ?? ""}`;
  const isOnline = runtimeState === "running";
  const natStatus = networkStatus?.natStatus ?? "unknown";
  const connectedPeers = networkStatus?.connectedPeers ?? 0;

  const [editing, setEditing] = useState(false);
  const [nameInput, setNameInput] = useState(displayName);
  const [copied, setCopied] = useState(false);
  const inputRef = useRef<TextInput>(null);

  useEffect(() => {
    if (!editing) setNameInput(displayName);
  }, [editing, displayName]);

  const handleSaveName = async () => {
    const trimmed = nameInput.trim();
    if (trimmed && trimmed !== deviceName) {
      try {
        await applyDeviceName(trimmed);
        toast.success(t`设备名称已更新`);
      } catch (err) {
        toast.error(errorMessage(err));
        return;
      }
    }
    setEditing(false);
  };

  const handleCopyPeerId = async () => {
    if (!peerId) return;
    await Clipboard.setStringAsync(peerId);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    setCopied(true);
    toast.success(t`已复制到剪贴板`);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <View className="overflow-hidden rounded-lg border border-border bg-card">
      <View className="flex-row items-center gap-4 p-4">
        {/* 头像 + 在线点 + 平台角标 */}
        <View className="relative">
          <View
            className={
              isOnline
                ? "absolute -left-1 -top-1 z-10 size-3.5 rounded-full border-2 border-background bg-success"
                : "absolute -left-1 -top-1 z-10 size-3.5 rounded-full border-2 border-background bg-muted-foreground"
            }
          />
          <View className="size-14 items-center justify-center rounded-2xl bg-primary/10">
            <Text className="text-xl font-bold text-primary">
              {avatarInitials}
            </Text>
          </View>
          <View className="absolute -bottom-1 -right-1 size-5 items-center justify-center rounded-lg border border-border bg-background">
            <Icon color={colors.mutedForeground} size={12} />
          </View>
        </View>

        {/* 设备信息 */}
        <View className="min-w-0 flex-1 gap-1">
          {editing ? (
            <TextInput
              ref={inputRef}
              value={nameInput}
              onChangeText={setNameInput}
              onBlur={handleSaveName}
              onSubmitEditing={handleSaveName}
              autoFocus
              returnKeyType="done"
              style={
                Platform.OS === "android"
                  ? { includeFontPadding: false }
                  : undefined
              }
              className="h-7 text-base font-bold text-foreground border-b border-primary px-0 py-0"
              placeholder={systemHostname}
              placeholderTextColor={colors.mutedForeground}
            />
          ) : (
            <Pressable
              onPress={() => {
                setNameInput(deviceName || systemHostname);
                setEditing(true);
              }}
              accessibilityRole="button"
              accessibilityLabel={t`编辑设备名`}
              hitSlop={4}
              className="flex-row items-center gap-1.5"
            >
              <Text
                className="flex-shrink text-base font-bold text-foreground"
                numberOfLines={1}
              >
                {displayName}
              </Text>
              <Pencil color={colors.mutedForeground} size={12} />
            </Pressable>
          )}

          {/* OS */}
          <View className="flex-row items-center gap-1.5">
            <Cpu color={colors.mutedForeground} size={12} />
            <Text
              className="flex-1 text-[12px] text-muted-foreground"
              numberOfLines={1}
            >
              {osLabel}
            </Text>
          </View>

          {/* PeerID */}
          <Pressable
            onPress={handleCopyPeerId}
            accessibilityRole="button"
            accessibilityLabel={t`复制 PeerID`}
            hitSlop={4}
            className="flex-row items-center gap-1.5"
          >
            <Activity color={colors.mutedForeground} size={12} />
            <Text
              className="flex-1 font-mono text-[12px] text-muted-foreground"
              numberOfLines={1}
            >
              {peerId ? truncateMiddle(peerId, 8, 4) : "—"}
            </Text>
            {copied ? (
              <Check color={colors.success} size={12} />
            ) : (
              <Copy color={colors.mutedForeground} size={12} />
            )}
          </Pressable>
        </View>
      </View>

      {/* 底部三列指标 */}
      <View className="flex-row border-t border-border">
        <Stat
          icon={Zap}
          label={<Trans>已连节点</Trans>}
          value={String(connectedPeers)}
        />
        <View className="w-px bg-border" />
        <Stat
          icon={ShieldCheck}
          label={<Trans>配对设备</Trans>}
          value={String(pairedDeviceCount)}
        />
        <View className="w-px bg-border" />
        <Stat
          icon={Activity}
          label={<Trans>NAT</Trans>}
          value={natStatus === "public" ? t`映射成功` : t`未知`}
        />
      </View>
    </View>
  );
}

function Stat({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof Zap;
  label: React.ReactNode;
  value: string;
}) {
  const colors = useThemeColors();
  return (
    <View className="flex-1 items-center gap-1 py-3">
      <View className="flex-row items-center gap-1">
        <Icon color={colors.mutedForeground} size={12} />
        <Text className="text-[11px] text-muted-foreground">{label}</Text>
      </View>
      <Text className="text-base font-bold text-foreground" numberOfLines={1}>
        {value}
      </Text>
    </View>
  );
}
