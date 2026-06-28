import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { Plus, Smartphone } from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { DeviceCard } from "@/components/device-card";
import {
  AppHeader,
  AppScreen,
  EmptyState,
  Surface,
} from "@/components/mobile/screen";
import {
  NodeControlSheet,
  type NodeControlSheetRef,
} from "@/components/node-control-sheet";
import { PairingSheet, type PairingSheetRef } from "@/components/pairing-sheet";
import { RecentTransferRow } from "@/components/recent-transfer-row";
import { StatusPill } from "@/components/status-pill";
import { Text } from "@/components/ui/text";
import { canSendToDevice } from "@/core/device-trust";
import { isProjectionActive } from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function DevicesScreen() {
  const router = useRouter();
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

  const projections = useTransferStore((s) => s.projections);
  const progressBySession = useTransferStore((s) => s.progressBySession);
  const loadProjections = useTransferStore((s) => s.loadProjections);

  useEffect(() => {
    if (!initialized) {
      void loadIdentity();
    }
  }, [initialized, loadIdentity]);

  useEffect(() => {
    void loadProjections();
  }, [loadProjections]);

  useEffect(() => {
    if (error !== null) {
      toast.error(error);
      setError(null);
    }
  }, [error, setError]);

  const pairedDevices = useMemo(() => {
    if (runtimeState === "running") {
      return devices.filter((d) => d.isPaired);
    }
    return summariesToOfflineDevices(pairedDevicesCache);
  }, [runtimeState, devices, pairedDevicesCache]);

  const activeProjections = useMemo(
    () =>
      Object.values(projections)
        .filter(isProjectionActive)
        .sort((a, b) => Number(b.updatedAt - a.updatedAt))
        .slice(0, 3),
    [projections],
  );

  const openDeviceDetail = useCallback(
    (device: DeviceInfo) => {
      router.push({
        pathname: "/device/[peerId]",
        params: { peerId: device.peerId },
      } as never);
    },
    [router],
  );

  const sendToDevice = useCallback(
    (device: DeviceInfo) => {
      if (!canSendToDevice(device)) {
        toast.info(t`设备当前不可发送`);
        return;
      }
      router.push({
        pathname: "/send/select-device",
        params: { peerId: device.peerId },
      } as never);
    },
    [router, t],
  );

  return (
    <AppScreen scroll testID="devices-screen" contentClassName="gap-5 pt-1">
      <AppHeader
        title={<Trans>设备</Trans>}
        subtitle={<Trans>发送文件、配对设备，并查看连接状态</Trans>}
        right={
          <View className="flex-row items-center gap-2">
            <StatusPill
              state={runtimeState}
              onPress={() => nodeSheetRef.current?.present()}
            />
          </View>
        }
        testID="devices-header"
      />

      <Surface className="gap-3" testID="devices-node-summary">
        <View className="flex-row items-center justify-between gap-3">
          <View className="min-w-0 flex-1">
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>移动节点</Trans>
            </Text>
            <Text className="mt-0.5 text-[12px] text-muted-foreground">
              {runtimeState === "running" ? (
                <Trans>附近设备可以发现这台手机</Trans>
              ) : (
                <Trans>启动节点后才能发现和连接设备</Trans>
              )}
            </Text>
          </View>
          <Pressable
            onPress={() => nodeSheetRef.current?.present()}
            accessibilityRole="button"
            testID="devices-manage-node-button"
            className="min-h-11 items-center justify-center rounded-xl border border-border px-3 active:opacity-70"
          >
            <Text className="text-[12px] font-semibold text-foreground">
              <Trans>管理</Trans>
            </Text>
          </Pressable>
        </View>
      </Surface>

      <View className="gap-3">
        <View className="flex-row items-center justify-between">
          <Text className="text-[15px] font-semibold text-foreground">
            <Trans>已配对设备</Trans>
            {pairedDevices.length > 0 ? ` (${pairedDevices.length})` : ""}
          </Text>
          <Pressable
            onPress={() => pairingSheetRef.current?.present()}
            accessibilityRole="button"
            testID="devices-add-device-button"
            className="min-h-11 flex-row items-center gap-1.5 rounded-xl bg-primary/10 px-3 active:opacity-70"
          >
            <Plus color={colors.primary} size={15} />
            <Text className="text-[12px] font-semibold text-primary">
              <Trans>添加</Trans>
            </Text>
          </Pressable>
        </View>

        {pairedDevices.length === 0 ? (
          <EmptyState
            icon={Smartphone}
            title={<Trans>还没有配对设备</Trans>}
            description={<Trans>通过附近发现或配对码连接你的第一台设备</Trans>}
            actionLabel={<Trans>添加设备</Trans>}
            onAction={() => pairingSheetRef.current?.present()}
            testID="devices-empty-state"
          />
        ) : (
          <DeviceGrid
            devices={pairedDevices}
            onPress={openDeviceDetail}
            onSend={sendToDevice}
          />
        )}
      </View>

      <View className="gap-3">
        <View className="flex-row items-center justify-between">
          <Text className="text-[15px] font-semibold text-foreground">
            <Trans>活跃传输</Trans>
          </Text>
          <Pressable
            onPress={() => router.push("/activity" as never)}
            accessibilityRole="button"
            hitSlop={8}
            testID="devices-open-activity-button"
          >
            <Text className="text-[12px] font-semibold text-primary">
              <Trans>查看活动</Trans>
            </Text>
          </Pressable>
        </View>
        {activeProjections.length > 0 ? (
          <View className="gap-2">
            {activeProjections.map((projection) => (
              <RecentTransferRow
                key={projection.sessionId}
                projection={projection}
                progress={progressBySession[projection.sessionId]}
                onPress={(sessionId) =>
                  router.push({
                    pathname: "/transfer/[sessionId]",
                    params: { sessionId },
                  } as never)
                }
              />
            ))}
          </View>
        ) : (
          <Surface className="py-5" testID="devices-empty-active-transfers">
            <Text className="text-center text-[12px] text-muted-foreground">
              <Trans>没有正在进行的传输</Trans>
            </Text>
          </Surface>
        )}
      </View>

      <PairingSheet ref={pairingSheetRef} />
      <NodeControlSheet ref={nodeSheetRef} />
    </AppScreen>
  );
}

function DeviceGrid({
  devices,
  onPress,
  onSend,
}: {
  devices: DeviceInfo[];
  onPress: (d: DeviceInfo) => void;
  onSend: (d: DeviceInfo) => void;
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
          {row.map((device, colIdx) => (
            <DeviceCard
              key={device.peerId}
              device={device}
              testID={`device-card-${idx * 2 + colIdx}`}
              onPress={onPress}
              onSend={onSend}
            />
          ))}
          {row.length === 1 ? <View className="flex-1" /> : null}
        </View>
      ))}
    </View>
  );
}
