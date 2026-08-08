/**
 * 分享目标 —— 选设备发送屏(反向流:文件已定、挑设备)。
 *
 * 入口:别的 App 分享 文件/图片/视频 → 根布局 ShareIntentHandler 映射成 TransferFile[]
 * 塞进 `share-store` → push 这里。流程:
 *   1. 顶部展示分享的文件(紧凑列表,可发前删单个)
 *   2. 中部选一台在线可发送的已配对设备(单选高亮);节点未启动自动启动
 *   3. 底部「发送给 X」→ 复用 `startSend` → `/transfer/[sessionId]`
 * 离屏(返回/发送成功)时清空 share-store。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import { type Href, useNavigation, useRouter } from "expo-router";
import { Check, Files, MonitorSmartphone, Send } from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, FlatList, Pressable, View } from "react-native";
import type {
  MobileDevice as DeviceInfo,
  MobilePrepareProgress,
} from "react-native-swarmdrop-core";
import { MobileCoreEvent_Tags } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { ConnectionBadge } from "@/components/connection-badge";
import {
  AppScreen,
  BottomActionBar,
  EmptyState,
  LIST_CONTENT_PADDING_UNDER_HEADER,
  ListItemGap,
  Surface,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import {
  calcPercent,
  formatBytes,
  ProgressBar,
} from "@/components/transfer/shared";
import { TrustBadge } from "@/components/trust-badge";
import { Text } from "@/components/ui/text";
import { canSendToDevice, resolveTrustLevel } from "@/core/device-trust";
import { subscribeCoreEvents } from "@/core/event-bus";
import { getMobileCore } from "@/core/mobile-core";
import { useThemeColors } from "@/hooks/useThemeColors";
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { getErrorMessage } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import {
  mergePairedDevicesWithCache,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useShareStore } from "@/stores/share-store";
import { useTransferStore } from "@/stores/transfer-store";

type ThemeColors = ReturnType<typeof useThemeColors>;

export default function ShareTargetScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const navigation = useNavigation();
  const colors = useThemeColors();

  const sharedFiles = useShareStore((s) => s.sharedFiles);
  const clearSharedFiles = useShareStore((s) => s.clearSharedFiles);

  const { devices, pairedDevicesCache, runtimeState, startNode } =
    useMobileCoreStore(
      useShallow((s) => ({
        devices: s.devices,
        pairedDevicesCache: s.pairedDevicesCache,
        runtimeState: s.runtimeState,
        startNode: s.startNode,
      })),
    );
  const startSend = useTransferStore((s) => s.startSend);

  const [selectedPeerId, setSelectedPeerId] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [prepareProgress, setPrepareProgress] =
    useState<MobilePrepareProgress | null>(null);

  // 当前 screen 真正从栈中移除时清理；push 到文件检查页不会触发 beforeRemove。
  useEffect(
    () => navigation.addListener("beforeRemove", clearSharedFiles),
    [clearSharedFiles, navigation],
  );

  // 准备阶段进度(与发送准备页一致,订阅 PrepareProgress 事件)。
  useEffect(() => {
    return subscribeCoreEvents((event) => {
      if (event.tag === MobileCoreEvent_Tags.PrepareProgress) {
        setPrepareProgress(event.inner.event);
      }
    });
  }, []);

  // 节点未启动时自动启动一次(分享进来常处于冷启动、节点还没起)。
  const startedRef = useRef(false);
  useEffect(() => {
    if (!startedRef.current && runtimeState === "stopped") {
      startedRef.current = true;
      void startNode();
    }
  }, [runtimeState, startNode]);

  // 在线且可发送的已配对设备(合并 keychain 缓存 + 实时发现)。
  const targetDevices = useMemo<DeviceInfo[]>(() => {
    return mergePairedDevicesWithCache(devices, pairedDevicesCache)
      .filter((d) => d.status === "online" && canSendToDevice(d))
      .sort((a, b) => deviceDisplayName(a).localeCompare(deviceDisplayName(b)));
  }, [devices, pairedDevicesCache]);

  // 选中的设备掉线/变不可发送 → 取消选中。
  useEffect(() => {
    if (
      selectedPeerId &&
      !targetDevices.some((d) => d.peerId === selectedPeerId)
    ) {
      setSelectedPeerId(null);
    }
  }, [targetDevices, selectedPeerId]);

  const selectedDevice =
    targetDevices.find((d) => d.peerId === selectedPeerId) ?? null;
  const totalSize = sharedFiles.reduce((sum, f) => sum + f.size, 0n);
  const canSend = !sending && sharedFiles.length > 0 && selectedDevice !== null;

  const openSharedFiles = useCallback(() => {
    router.push("/send/shared-files" as Href);
  }, [router]);

  const onSend = useCallback(async () => {
    if (!selectedDevice || sending || sharedFiles.length === 0) return;
    setSending(true);
    setPrepareProgress(null);
    try {
      const sessionId = await startSend({
        files: sharedFiles,
        peerId: selectedDevice.peerId,
        peerName: deviceDisplayName(selectedDevice),
      });
      clearSharedFiles();
      router.replace({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
      });
    } catch (err) {
      let panicDetail: string | undefined;
      try {
        panicDetail = getMobileCore().takeLastPanic() ?? undefined;
      } catch {}
      toast.error(t`发送失败`, panicDetail ?? getErrorMessage(err));
    } finally {
      setSending(false);
      setPrepareProgress(null);
    }
  }, [
    selectedDevice,
    sending,
    sharedFiles,
    startSend,
    clearSharedFiles,
    router,
    t,
  ]);

  return (
    <AppScreen header={<SettingsHeader title={t`发送到`} />} bare>
      <FlatList
        data={targetDevices}
        keyExtractor={(device) => device.peerId}
        renderItem={({ item }) => (
          <TargetDeviceRow
            device={item}
            selected={item.peerId === selectedPeerId}
            colors={colors}
            onPress={() => setSelectedPeerId(item.peerId)}
          />
        )}
        ItemSeparatorComponent={ListItemGap}
        contentContainerStyle={SHARE_TARGET_CONTENT_STYLE}
        ListHeaderComponent={
          <View className="gap-4 pb-3">
            <Surface className="gap-3">
              <View className="flex-row items-center gap-3">
                <View className="size-11 items-center justify-center rounded-xl bg-muted">
                  <Files color={colors.foreground} size={20} />
                </View>
                <View className="min-w-0 flex-1">
                  <Text className="text-[15px] font-semibold text-foreground">
                    <Trans>{sharedFiles.length} 个文件</Trans>
                  </Text>
                  <Text className="text-[13px] text-muted-foreground">
                    {formatBytes(totalSize)}
                  </Text>
                </View>
                <Pressable
                  onPress={openSharedFiles}
                  disabled={sharedFiles.length === 0}
                  accessibilityRole="button"
                  accessibilityLabel={t`查看分享文件`}
                  testID="share-target-review-files"
                  className="min-h-11 justify-center rounded-xl border border-border px-3.5 active:opacity-70 disabled:opacity-50"
                >
                  <Text className="text-[13px] font-semibold text-foreground">
                    <Trans>查看文件</Trans>
                  </Text>
                </Pressable>
              </View>
            </Surface>
            <Text className="text-[13px] font-semibold text-muted-foreground">
              <Trans>选择设备</Trans>
            </Text>
          </View>
        }
        ListEmptyComponent={
          runtimeState !== "running" ? (
            <View className="min-h-32 items-center justify-center gap-3 rounded-lg border border-dashed border-border bg-card py-8">
              <ActivityIndicator color={colors.mutedForeground} />
              <View className="items-center gap-1">
                <Text className="text-[13px] font-semibold text-foreground">
                  <Trans>正在启动节点…</Trans>
                </Text>
                <Text className="text-[12px] text-muted-foreground">
                  <Trans>启动后显示可发送的设备</Trans>
                </Text>
              </View>
            </View>
          ) : (
            <EmptyState
              icon={MonitorSmartphone}
              title={<Trans>没有在线设备</Trans>}
              description={
                <Trans>
                  让目标设备打开 SwarmDrop 并保持在线，或先配对一台设备。
                </Trans>
              }
            />
          )
        }
        testID="share-target-device-list"
      />

      {/* 底部发送栏 */}
      <BottomActionBar testID="share-target-action-bar">
        {sending && prepareProgress ? (
          <SharePrepareProgress progress={prepareProgress} />
        ) : (
          <Pressable
            onPress={onSend}
            disabled={!canSend}
            accessibilityRole="button"
            accessibilityLabel={
              selectedDevice
                ? t`发送给 ${deviceDisplayName(selectedDevice)}`
                : t`选择一个设备`
            }
            accessibilityState={{ busy: sending, disabled: !canSend }}
            className="min-h-12 flex-1 flex-row items-center justify-center gap-2 rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-50"
          >
            {sending ? (
              <ActivityIndicator
                color={colors.primaryForeground}
                size="small"
              />
            ) : (
              <Send color={colors.primaryForeground} size={18} />
            )}
            <Text
              className="shrink text-[14px] font-semibold text-primary-foreground"
              numberOfLines={1}
            >
              {sending ? (
                <Trans>准备中</Trans>
              ) : selectedDevice ? (
                <Trans>发送给 {deviceDisplayName(selectedDevice)}</Trans>
              ) : (
                <Trans>选择一个设备</Trans>
              )}
            </Text>
          </Pressable>
        )}
      </BottomActionBar>
    </AppScreen>
  );
}

/* ─── 可选目标设备行 ─── */

// 派生自共享常量,别再抄 20 这个数:横向锚点与顶距跟着 header 契约走,
// 只有底部因为下方压着 BottomActionBar 才收窄(不需要那 32 的滚动余量)。
const SHARE_TARGET_CONTENT_STYLE = {
  ...LIST_CONTENT_PADDING_UNDER_HEADER,
  flexGrow: 1,
  paddingBottom: 16,
} as const;

function TargetDeviceRow({
  device,
  selected,
  colors,
  onPress,
}: {
  device: DeviceInfo;
  selected: boolean;
  colors: ThemeColors;
  onPress: () => void;
}) {
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const trustLevel = resolveTrustLevel(device);
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityState={{ selected }}
      accessibilityLabel={deviceDisplayName(device)}
      className={cn(
        "min-h-16 flex-row items-center gap-3 rounded-lg border p-3.5 active:opacity-70",
        selected ? "border-primary bg-primary/5" : "border-border bg-card",
      )}
    >
      <View className="size-10 items-center justify-center rounded-full bg-muted">
        <Icon color={colors.foreground} size={19} />
      </View>
      <View className="min-w-0 flex-1 gap-1">
        <Text
          className="text-[14px] font-semibold text-foreground"
          numberOfLines={1}
        >
          {deviceDisplayName(device)}
        </Text>
        <View className="min-w-0 flex-row flex-wrap items-center gap-1.5">
          <View className="size-1.5 rounded-full bg-success" />
          <Text className="text-[12px] text-success-ink">
            <Trans>在线</Trans>
          </Text>
          <TrustBadge
            level={trustLevel}
            compact
            confirmed={device.trustConfirmed}
          />
          <ConnectionBadge
            connection={device.connection}
            latencyMs={device.latencyMs}
            compact
          />
        </View>
      </View>
      <View
        className={cn(
          "size-6 items-center justify-center rounded-full border",
          selected ? "border-primary bg-primary" : "border-border",
        )}
      >
        {selected ? <Check size={14} color={colors.primaryForeground} /> : null}
      </View>
    </Pressable>
  );
}

/* ─── 底部准备进度(与发送准备页一致) ─── */

function SharePrepareProgress({
  progress,
}: {
  progress: MobilePrepareProgress;
}) {
  const total = Number(progress.totalBytes);
  const hashed = Number(progress.bytesHashed);
  return (
    <View className="flex-1 gap-2 py-1">
      <View className="flex-row items-center justify-between gap-3">
        <Text
          className="flex-1 text-[13px] text-muted-foreground"
          numberOfLines={1}
        >
          <Trans>
            正在准备 ({progress.completedFiles.toString()}/
            {progress.totalFiles.toString()})
          </Trans>
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          {formatBytes(hashed)} / {formatBytes(total)}
        </Text>
      </View>
      <ProgressBar percent={calcPercent(hashed, total)} heightClass="h-1.5" />
    </View>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
