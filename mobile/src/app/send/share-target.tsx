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
import { useRouter } from "expo-router";
import {
  Check,
  File as FileIcon,
  Image as ImageIcon,
  Inbox,
  type LucideIcon,
  Send,
  Video,
  X,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type {
  MobileDevice as DeviceInfo,
  MobilePrepareProgress,
  MobileTransferFile as TransferFile,
} from "react-native-swarmdrop-core";
import { MobileCoreEvent_Tags } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { ConnectionBadge } from "@/components/connection-badge";
import { EmptyState, Surface } from "@/components/mobile/screen";
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
import { toast } from "@/lib/toast";
import { cn, errorMessage } from "@/lib/utils";
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
  const colors = useThemeColors();

  const sharedFiles = useShareStore((s) => s.sharedFiles);
  const removeSharedByPath = useShareStore((s) => s.removeSharedByPath);
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

  // 离屏(返回/发送成功后 replace)统一清空在途分享,避免下次进入带上旧文件。
  useEffect(() => () => clearSharedFiles(), [clearSharedFiles]);

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
      toast.error(t`发送失败`, panicDetail ?? errorMessage(err));
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
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`发送到`} />

      <ScrollView
        contentContainerClassName="gap-4 px-5 pt-2 pb-4"
        showsVerticalScrollIndicator={false}
      >
        {/* ① 分享的文件 */}
        <Surface className="gap-3">
          <View className="flex-row items-center justify-between gap-3">
            <Text className="text-[15px] font-semibold text-foreground">
              <Trans>{sharedFiles.length} 个文件</Trans>
            </Text>
            <Text className="text-[12px] text-muted-foreground">
              {formatBytes(Number(totalSize))}
            </Text>
          </View>
          {sharedFiles.length > 0 ? (
            <View className="gap-1">
              {sharedFiles.map((file) => (
                <SharedFileRow
                  key={file.relativePath}
                  file={file}
                  colors={colors}
                  onRemove={() => removeSharedByPath(file.relativePath)}
                  removeLabel={t`移除`}
                />
              ))}
            </View>
          ) : (
            <Text className="text-[12px] text-muted-foreground">
              <Trans>文件已全部移除，返回重新分享。</Trans>
            </Text>
          )}
        </Surface>

        {/* ② 选择设备 */}
        <View className="gap-2.5">
          <Text className="text-[13px] font-semibold text-muted-foreground">
            <Trans>选择设备</Trans>
          </Text>
          {runtimeState !== "running" ? (
            <View className="min-h-32 items-center justify-center gap-3 rounded-lg border border-dashed border-border bg-card py-8">
              <ActivityIndicator color={colors.mutedForeground} />
              <View className="items-center gap-1">
                <Text className="text-[13px] font-semibold text-foreground">
                  <Trans>正在启动节点…</Trans>
                </Text>
                <Text className="text-[11px] text-muted-foreground">
                  <Trans>启动后显示可发送的设备</Trans>
                </Text>
              </View>
            </View>
          ) : targetDevices.length === 0 ? (
            <EmptyState
              icon={Inbox}
              title={<Trans>没有在线设备</Trans>}
              description={
                <Trans>
                  让目标设备打开 SwarmDrop 并保持在线，或先配对一台设备。
                </Trans>
              }
            />
          ) : (
            <View className="gap-2">
              {targetDevices.map((device) => (
                <TargetDeviceRow
                  key={device.peerId}
                  device={device}
                  selected={device.peerId === selectedPeerId}
                  colors={colors}
                  onPress={() => setSelectedPeerId(device.peerId)}
                />
              ))}
            </View>
          )}
        </View>
      </ScrollView>

      {/* 底部发送栏 */}
      <View className="gap-2 border-t border-border bg-card px-5 py-3">
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
            className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
          >
            {sending ? (
              <ActivityIndicator
                color={colors.primaryForeground}
                size="small"
              />
            ) : (
              <Send color={colors.primaryForeground} size={17} />
            )}
            <Text className="text-[14px] font-semibold text-primary-foreground">
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
      </View>
    </SafeAreaView>
  );
}

/* ─── 分享文件行 ─── */

const IMAGE_EXTS = [
  ".jpg",
  ".jpeg",
  ".png",
  ".gif",
  ".webp",
  ".heic",
  ".heif",
  ".bmp",
  ".tiff",
  ".avif",
];
const VIDEO_EXTS = [
  ".mp4",
  ".mov",
  ".m4v",
  ".webm",
  ".mkv",
  ".avi",
  ".wmv",
  ".flv",
  ".3gp",
];

function fileIconFor(name: string): LucideIcon {
  const lower = name.toLowerCase();
  if (IMAGE_EXTS.some((e) => lower.endsWith(e))) return ImageIcon;
  if (VIDEO_EXTS.some((e) => lower.endsWith(e))) return Video;
  return FileIcon;
}

function SharedFileRow({
  file,
  colors,
  onRemove,
  removeLabel,
}: {
  file: TransferFile;
  colors: ThemeColors;
  onRemove: () => void;
  removeLabel: string;
}) {
  const Icon = fileIconFor(file.name);
  return (
    <View className="flex-row items-center gap-3 py-1.5">
      <Icon color={colors.mutedForeground} size={18} />
      <Text
        className="min-w-0 flex-1 text-[13px] text-foreground"
        numberOfLines={1}
      >
        {file.name}
      </Text>
      <Text className="text-[11px] text-muted-foreground">
        {formatBytes(Number(file.size))}
      </Text>
      <Pressable
        onPress={onRemove}
        hitSlop={10}
        accessibilityRole="button"
        accessibilityLabel={removeLabel}
        className="rounded-full p-1 active:bg-destructive/15"
      >
        <X size={14} color={colors.mutedForeground} />
      </Pressable>
    </View>
  );
}

/* ─── 可选目标设备行 ─── */

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
        "min-h-[64px] flex-row items-center gap-3 rounded-lg border p-3 active:opacity-70",
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
          <Text className="text-[11px] text-success-ink">
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
    <View className="gap-2 py-1">
      <View className="flex-row items-center justify-between gap-3">
        <Text
          className="flex-1 text-[12px] text-muted-foreground"
          numberOfLines={1}
        >
          <Trans>
            正在准备 ({progress.completedFiles.toString()}/
            {progress.totalFiles.toString()})
          </Trans>
        </Text>
        <Text className="text-[11px] text-muted-foreground">
          {formatBytes(hashed)} / {formatBytes(total)}
        </Text>
      </View>
      <ProgressBar percent={calcPercent(hashed, total)} heightClass="h-1.5" />
    </View>
  );
}
