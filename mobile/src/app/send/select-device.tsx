import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { Plus, Power, Send, Smartphone, WifiOff } from "lucide-react-native";
import { useMemo, useRef, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  NodeControlSheet,
  type NodeControlSheetRef,
} from "@/components/node-control-sheet";
import { PairingSheet, type PairingSheetRef } from "@/components/pairing-sheet";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { useThemeColors } from "@/hooks/useThemeColors";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import { errorMessage } from "@/lib/utils";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function SelectDevice() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const pairingSheetRef = useRef<PairingSheetRef>(null);
  const nodeSheetRef = useRef<NodeControlSheetRef>(null);

  const {
    devices,
    pairedDevicesCache,
    runtimeState,
    selectedFiles,
    clearSelectedFiles,
  } = useMobileCoreStore(
    useShallow((s) => ({
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
      runtimeState: s.runtimeState,
      selectedFiles: s.selectedFiles,
      clearSelectedFiles: s.clearSelectedFiles,
    })),
  );
  const registerSession = useTransferStore((s) => s.registerSession);

  const [sendingTo, setSendingTo] = useState<string | null>(null);

  // 节点 running 时用实时 devices,否则 fallback cache(全离线)
  const pairedDevices = useMemo<DeviceInfo[]>(() => {
    if (runtimeState === "running") {
      return devices.filter((d) => d.isPaired);
    }
    return summariesToOfflineDevices(pairedDevicesCache);
  }, [runtimeState, devices, pairedDevicesCache]);

  const onlineCount = pairedDevices.filter((d) => d.status === "online").length;
  const totalSize = selectedFiles.reduce((s, f) => s + f.size, 0n);

  const onSend = async (peerId: string, peerName: string) => {
    if (sendingTo !== null || selectedFiles.length === 0) return;
    setSendingTo(peerId);
    try {
      const prepared = await getMobileCore().prepareSend(selectedFiles);
      const result = await getMobileCore().sendPrepared(
        prepared.preparedId,
        peerId,
        peerName,
        [],
      );
      registerSession(result.sessionId);
      clearSelectedFiles();
      router.back();
    } catch (err) {
      toast.error(t`发送失败`, errorMessage(err));
    } finally {
      setSendingTo(null);
    }
  };

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`选择接收设备`} />

      <View className="px-5 pt-2">
        <View className="rounded-xl bg-primary/10 p-3.5">
          <Text className="text-[14px] font-semibold text-foreground">
            <Trans>{selectedFiles.length} 个文件</Trans>
          </Text>
          <Text className="text-[12px] text-muted-foreground mt-0.5">
            {formatBytes(totalSize)}
          </Text>
        </View>
      </View>

      {runtimeState !== "running" && pairedDevices.length > 0 ? (
        <View className="px-5 pt-3">
          <Pressable
            onPress={() => nodeSheetRef.current?.present()}
            accessibilityRole="button"
            className="flex-row items-center gap-2 rounded-xl border border-warning/30 bg-warning/10 px-3.5 py-2.5 active:opacity-70"
          >
            <Power color={colors.warning} size={16} />
            <Text className="flex-1 text-[12px] font-medium text-warning">
              <Trans>节点未启动,启动后才能发送</Trans>
            </Text>
            <Text className="text-[12px] font-semibold text-warning">
              <Trans>启动</Trans>
            </Text>
          </Pressable>
        </View>
      ) : null}

      <ScrollView contentContainerClassName="gap-2 px-5 pt-3 pb-6">
        {pairedDevices.length === 0 ? (
          <EmptyNoPaired
            onAdd={() => pairingSheetRef.current?.present()}
            onBack={() => router.back()}
          />
        ) : onlineCount === 0 && runtimeState === "running" ? (
          <EmptyAllOffline onBack={() => router.back()} />
        ) : (
          pairedDevices.map((d) => (
            <DeviceRow
              key={d.peerId}
              device={d}
              sending={sendingTo === d.peerId}
              disabled={sendingTo !== null || d.status !== "online"}
              onPress={() => onSend(d.peerId, d.hostname)}
            />
          ))
        )}
      </ScrollView>

      <PairingSheet ref={pairingSheetRef} />
      <NodeControlSheet ref={nodeSheetRef} />
    </SafeAreaView>
  );
}

function DeviceRow({
  device,
  sending,
  disabled,
  onPress,
}: {
  device: DeviceInfo;
  sending: boolean;
  disabled: boolean;
  onPress: () => void;
}) {
  const colors = useThemeColors();
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const isOnline = device.status === "online";

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityLabel={device.hostname}
      className="flex-row items-center gap-3 rounded-xl border border-border bg-card p-3.5 active:opacity-70 disabled:opacity-50"
    >
      <View className="size-9 items-center justify-center rounded-full bg-muted">
        <Icon color={colors.foreground} size={18} />
      </View>
      <View className="flex-1 gap-0.5">
        <Text
          className="text-[14px] font-semibold text-foreground"
          numberOfLines={1}
        >
          {device.hostname}
        </Text>
        <View className="flex-row items-center gap-1">
          <View
            className={
              isOnline
                ? "size-1.5 rounded-full bg-success"
                : "size-1.5 rounded-full bg-muted-foreground"
            }
          />
          <Text
            className={
              isOnline
                ? "text-[11px] text-success"
                : "text-[11px] text-muted-foreground"
            }
          >
            {isOnline ? <Trans>在线</Trans> : <Trans>离线</Trans>}
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            · {device.platform}
          </Text>
        </View>
      </View>
      {sending ? (
        <ActivityIndicator color={colors.primary} />
      ) : (
        <Send
          color={isOnline ? colors.primary : colors.mutedForeground}
          size={16}
        />
      )}
    </Pressable>
  );
}

function EmptyNoPaired({
  onAdd,
  onBack,
}: {
  onAdd: () => void;
  onBack: () => void;
}) {
  const colors = useThemeColors();
  return (
    <View className="items-center gap-4 py-16">
      <View className="size-16 items-center justify-center rounded-full bg-muted">
        <Smartphone color={colors.mutedForeground} size={32} />
      </View>
      <View className="gap-1 items-center px-6">
        <Text className="text-[15px] font-semibold text-foreground">
          <Trans>还没有配对设备</Trans>
        </Text>
        <Text className="text-center text-[13px] text-muted-foreground">
          <Trans>添加设备后即可向其发送已选文件</Trans>
        </Text>
      </View>
      <View className="flex-row gap-2.5">
        <Pressable
          onPress={onBack}
          accessibilityRole="button"
          className="h-10 flex-row items-center justify-center rounded-xl border border-border bg-card px-4 active:opacity-70"
        >
          <Text className="text-[13px] text-foreground">
            <Trans>返回主页</Trans>
          </Text>
        </Pressable>
        <Pressable
          onPress={onAdd}
          accessibilityRole="button"
          className="h-10 flex-row items-center gap-1.5 rounded-xl bg-primary px-4 active:opacity-70"
        >
          <Plus color={colors.background} size={14} />
          <Text className="text-[13px] font-semibold text-primary-foreground">
            <Trans>添加设备</Trans>
          </Text>
        </Pressable>
      </View>
    </View>
  );
}

function EmptyAllOffline({ onBack }: { onBack: () => void }) {
  const colors = useThemeColors();
  return (
    <View className="items-center gap-4 py-16">
      <View className="size-16 items-center justify-center rounded-full bg-muted">
        <WifiOff color={colors.mutedForeground} size={32} />
      </View>
      <View className="gap-1 items-center px-6">
        <Text className="text-[15px] font-semibold text-foreground">
          <Trans>所有设备都离线</Trans>
        </Text>
        <Text className="text-center text-[13px] text-muted-foreground">
          <Trans>请确保对方设备已启动 SwarmDrop 并连入网络</Trans>
        </Text>
      </View>
      <Pressable
        onPress={onBack}
        accessibilityRole="button"
        className="h-10 flex-row items-center justify-center rounded-xl border border-border bg-card px-4 active:opacity-70"
      >
        <Text className="text-[13px] text-foreground">
          <Trans>返回主页</Trans>
        </Text>
      </Pressable>
    </View>
  );
}

function formatBytes(bytes: bigint): string {
  const n = Number(bytes);
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
