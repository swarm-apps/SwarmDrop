import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { DrawerActions, useNavigation } from "expo-router/react-navigation";
import { Menu, Plus, Smartphone } from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { DeviceCard } from "@/components/device-card";
import {
  NodeControlSheet,
  type NodeControlSheetRef,
} from "@/components/node-control-sheet";
import { PairingSheet, type PairingSheetRef } from "@/components/pairing-sheet";
import { RecentTransferRow } from "@/components/recent-transfer-row";
import { StatusPill } from "@/components/status-pill";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function HomeScreen() {
  const router = useRouter();
  const navigation = useNavigation();
  const colors = useThemeColors();
  const { t } = useLingui();
  const pairingSheetRef = useRef<PairingSheetRef>(null);
  const nodeSheetRef = useRef<NodeControlSheetRef>(null);

  const {
    devices,
    pairedDevicesCache,
    runtimeState,
    error,
    loadIdentity,
    initialized,
    setError,
  } = useMobileCoreStore(
    useShallow((s) => ({
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
      runtimeState: s.runtimeState,
      error: s.error,
      loadIdentity: s.loadIdentity,
      initialized: s.initialized,
      setError: s.setError,
    })),
  );

  const sessions = useTransferStore((s) => s.sessions);

  // 仅第一次进入主屏时加载身份(拿 peerId)。
  // 是否自动启动节点由 preferences.autoStart 决定,逻辑封装在 loadIdentity 内部。
  useEffect(() => {
    if (!initialized) {
      void loadIdentity();
    }
  }, [initialized, loadIdentity]);

  useEffect(() => {
    if (error !== null) {
      toast.error(error);
      setError(null);
    }
  }, [error, setError]);

  // 节点 running 时用实时 devices;否则 fallback 到持久化的 paired 骨架(显示离线)
  const pairedDevices = useMemo(() => {
    if (runtimeState === "running") {
      return devices.filter((d) => d.isPaired);
    }
    return summariesToOfflineDevices(pairedDevicesCache);
  }, [runtimeState, devices, pairedDevicesCache]);

  const activeSnapshots = useMemo(
    () =>
      Object.values(sessions)
        .sort((a, b) => b.startedAt - a.startedAt)
        .map((s) => s.progress)
        .filter((p): p is NonNullable<typeof p> => p !== null),
    [sessions],
  );

  const onDevicePress = useCallback(
    (device: DeviceInfo) => {
      if (device.status !== "online") {
        toast.info(t`设备离线,暂无法发送`);
        return;
      }
      // 直接跳「发送准备页」—— 用户在那里选文件 / 文件夹 / 照片 / 视频，
      // 可多次累加 / 撤销，最后点发送。对齐桌面端「点设备 → 发送页」的流程。
      router.push({
        pathname: "/send/select-device",
        params: { peerId: device.peerId },
      });
    },
    [router, t],
  );

  const openDrawer = () => navigation.dispatch(DrawerActions.openDrawer());

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <View className="flex-row items-center justify-between px-5 py-3">
        <Pressable
          onPress={openDrawer}
          hitSlop={12}
          accessibilityLabel={t`打开侧栏`}
          accessibilityRole="button"
        >
          <Menu color={colors.foreground} size={22} />
        </Pressable>
        <Text className="text-base font-semibold text-foreground">
          SwarmDrop
        </Text>
        <StatusPill
          state={runtimeState}
          onPress={() => nodeSheetRef.current?.present()}
        />
      </View>

      <ScrollView contentContainerClassName="flex-grow gap-5 px-5 pb-8">
        <View className="gap-3">
          <View className="flex-row items-center justify-between">
            <Text className="text-sm font-semibold text-foreground">
              <Trans>我的设备</Trans>
              {pairedDevices.length > 0 ? ` (${pairedDevices.length})` : ""}
            </Text>
            <Pressable
              onPress={() => pairingSheetRef.current?.present()}
              accessibilityRole="button"
              className="flex-row items-center gap-1 rounded-full bg-primary/10 px-3 py-1.5 active:opacity-70"
            >
              <Plus color={colors.primary} size={14} />
              <Text className="text-xs font-semibold text-primary">
                <Trans>添加</Trans>
              </Text>
            </Pressable>
          </View>

          {pairedDevices.length === 0 ? (
            <EmptyDevices onAdd={() => pairingSheetRef.current?.present()} />
          ) : (
            <DeviceGrid devices={pairedDevices} onPress={onDevicePress} />
          )}
        </View>

        {activeSnapshots.length > 0 ? (
          <View className="gap-3">
            <View className="flex-row items-center justify-between">
              <Text className="text-sm font-semibold text-foreground">
                <Trans>正在传输</Trans>
              </Text>
              <Pressable
                onPress={() => router.push("/transfer" as never)}
                accessibilityRole="button"
                hitSlop={6}
              >
                <Text className="text-xs font-medium text-primary">
                  <Trans>查看全部</Trans>
                </Text>
              </Pressable>
            </View>
            <View className="gap-2">
              {activeSnapshots.map((snap) => (
                <RecentTransferRow
                  key={snap.sessionId}
                  snapshot={snap}
                  onPress={(sid) =>
                    router.push({
                      pathname: "/transfer/[sessionId]",
                      params: { sessionId: sid },
                    } as never)
                  }
                />
              ))}
            </View>
          </View>
        ) : null}
      </ScrollView>

      <PairingSheet ref={pairingSheetRef} />
      <NodeControlSheet ref={nodeSheetRef} />
    </SafeAreaView>
  );
}

function DeviceGrid({
  devices,
  onPress,
}: {
  devices: DeviceInfo[];
  onPress: (d: DeviceInfo) => void;
}) {
  const rows = useMemo(() => {
    const result: DeviceInfo[][] = [];
    for (let i = 0; i < devices.length; i += 2) {
      result.push(devices.slice(i, i + 2));
    }
    return result;
  }, [devices]);

  return (
    <View className="gap-2.5">
      {rows.map((row, idx) => (
        <View key={String(idx)} className="flex-row gap-2.5">
          {row.map((d) => (
            <DeviceCard key={d.peerId} device={d} onPress={onPress} />
          ))}
          {row.length === 1 ? <View className="flex-1" /> : null}
        </View>
      ))}
    </View>
  );
}

function EmptyDevices({ onAdd }: { onAdd: () => void }) {
  const colors = useThemeColors();
  return (
    <View className="items-center gap-4 rounded-xl border border-dashed border-border bg-card py-10">
      <View className="size-14 items-center justify-center rounded-full bg-muted">
        <Smartphone color={colors.mutedForeground} size={28} />
      </View>
      <View className="gap-1 items-center">
        <Text className="text-sm font-semibold text-foreground">
          <Trans>还没有配对设备</Trans>
        </Text>
        <Text className="text-xs text-muted-foreground">
          <Trans>通过配对码连接你的第一台设备</Trans>
        </Text>
      </View>
      <Pressable
        onPress={onAdd}
        accessibilityRole="button"
        className="flex-row items-center gap-1.5 rounded-full bg-primary px-4 py-2 active:opacity-70"
      >
        <Plus color={colors.background} size={14} />
        <Text className="text-xs font-semibold text-primary-foreground">
          <Trans>添加设备</Trans>
        </Text>
      </Pressable>
    </View>
  );
}
