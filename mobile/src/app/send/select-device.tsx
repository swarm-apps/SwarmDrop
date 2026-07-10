/**
 * 发送准备页 —— 与桌面端 `/_app/send` 对齐。
 *
 * 入口：(main) 点击在线设备 → push 到这里（携带 peerId）。
 * 流程：
 *   1. 顶部 device header（不再让用户选设备，已经在主屏选过）
 *   2. 「添加文件 / 添加文件夹 / 照片 / 视频」按钮组，多次点击累加
 *   3. FileBrowser 渲染已选；点 X 移除单个文件 / 子目录
 *   4. 底部「取消 / 发送」操作栏；发送过程显示 prepareSend 进度条
 *   5. 发送成功 → router.replace 到 `/transfer/[sessionId]` 看实时进度
 */

import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams, useRouter } from "expo-router";
import {
  FileText,
  Folder,
  Image as ImageIcon,
  type LucideIcon,
  Video,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Alert, Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type {
  MobileDevice as DeviceInfo,
  MobilePrepareProgress,
} from "react-native-swarmdrop-core";
import { MobileCoreEvent_Tags } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  FileBrowser,
  type FileBrowserActions,
  fromSelectedFiles,
} from "@/components/file-browser";
import { BottomActionBar } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import {
  calcPercent,
  formatBytes,
  ProgressBar,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { canSendToDevice, resolveTrustLevel } from "@/core/device-trust";
import { subscribeCoreEvents } from "@/core/event-bus";
import {
  pickFromMediaLibrary,
  pickTransferDirectory,
  pickTransferFiles,
} from "@/core/file-access";
import { getMobileCore } from "@/core/mobile-core";
import { useThemeColors } from "@/hooks/useThemeColors";
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import { cn, errorMessage } from "@/lib/utils";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function SendPreparePage() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const { peerId } = useLocalSearchParams<{ peerId: string }>();

  const {
    devices,
    pairedDevicesCache,
    runtimeState,
    selectedFiles,
    appendFiles,
    removeSelectedBySourceId,
    removeSelectedDirectory,
    clearSelectedFiles,
  } = useMobileCoreStore(
    useShallow((s) => ({
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
      runtimeState: s.runtimeState,
      selectedFiles: s.selectedFiles,
      appendFiles: s.appendFiles,
      removeSelectedBySourceId: s.removeSelectedBySourceId,
      removeSelectedDirectory: s.removeSelectedDirectory,
      clearSelectedFiles: s.clearSelectedFiles,
    })),
  );
  const startSend = useTransferStore((s) => s.startSend);

  // 进入页面时清掉残留的旧选择（用户从主屏不同设备来回切换时不要带上次的）
  useEffect(() => {
    clearSelectedFiles();
    return () => {
      clearSelectedFiles();
    };
  }, [clearSelectedFiles]);

  const device = useMemo<DeviceInfo | null>(() => {
    if (!peerId) return null;
    const online = devices.find((d) => d.peerId === peerId);
    if (online) return online;
    const fallback = summariesToOfflineDevices(pairedDevicesCache).find(
      (d) => d.peerId === peerId,
    );
    return fallback ?? null;
  }, [peerId, devices, pairedDevicesCache]);

  const displayName = device ? deviceDisplayName(device) : "";

  // ── 准备阶段进度（订阅 PrepareProgress 事件）─────────────────
  const [sending, setSending] = useState(false);
  const [prepareProgress, setPrepareProgress] =
    useState<MobilePrepareProgress | null>(null);

  useEffect(() => {
    return subscribeCoreEvents((event) => {
      if (event.tag === MobileCoreEvent_Tags.PrepareProgress) {
        setPrepareProgress(event.inner.event);
      }
    });
  }, []);

  const browserItems = useMemo(
    () => fromSelectedFiles(selectedFiles),
    [selectedFiles],
  );
  const browserActions = useMemo<FileBrowserActions>(
    () => ({
      removeItem: (item) => {
        if (item.sourceId) removeSelectedBySourceId(item.sourceId);
      },
      removeDirectory: removeSelectedDirectory,
    }),
    [removeSelectedBySourceId, removeSelectedDirectory],
  );

  const totalSize = selectedFiles.reduce((s, f) => s + f.size, 0n);

  // ── 添加来源 handlers ──────────────────────────────────────
  const handlePick = useCallback(
    async (kind: "files" | "directory" | "photos" | "videos") => {
      try {
        const files =
          kind === "files"
            ? await pickTransferFiles()
            : kind === "directory"
              ? await pickTransferDirectory()
              : await pickFromMediaLibrary(kind);
        if (files.length > 0) appendFiles(files);
      } catch (err) {
        toast.error(t`选择失败`, errorMessage(err));
      }
    },
    [appendFiles, t],
  );

  // ── 发送 ───────────────────────────────────────────────────
  const onSend = useCallback(async () => {
    if (!device || sending || selectedFiles.length === 0) return;
    setSending(true);
    setPrepareProgress(null);
    try {
      const sessionId = await startSend({
        files: selectedFiles,
        peerId: device.peerId,
        peerName: displayName,
      });
      clearSelectedFiles();
      router.replace({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
      });
    } catch (err) {
      // uniffi panic 被包装成固定字符串 "Rust panic"，take_last_panic 拉详情。
      let panicDetail: string | undefined;
      try {
        panicDetail = getMobileCore().takeLastPanic() ?? undefined;
      } catch {}
      console.error("[send-prepare] send failed:", err, panicDetail);
      toast.error(t`发送失败`, panicDetail ?? errorMessage(err));
    } finally {
      setSending(false);
      setPrepareProgress(null);
    }
  }, [
    device,
    sending,
    selectedFiles,
    displayName,
    startSend,
    clearSelectedFiles,
    router,
    t,
  ]);

  const onCancel = useCallback(() => {
    if (selectedFiles.length > 0) {
      Alert.alert(t`放弃选择？`, t`已选的文件将被清空。`, [
        { text: t`继续选择`, style: "cancel" },
        {
          text: t`放弃`,
          style: "destructive",
          onPress: () => {
            clearSelectedFiles();
            router.back();
          },
        },
      ]);
    } else {
      router.back();
    }
  }, [selectedFiles.length, clearSelectedFiles, router, t]);

  // ── 渲染 ───────────────────────────────────────────────────
  if (!device) {
    return (
      <SafeAreaView
        style={{ flex: 1 }}
        className="bg-background"
        edges={["top"]}
      >
        <SettingsHeader title={t`发送`} />
        <View className="flex-1 items-center justify-center gap-3 px-6">
          <Text className="text-sm text-muted-foreground">
            <Trans>设备未找到</Trans>
          </Text>
          <Pressable
            onPress={() => router.back()}
            accessibilityRole="button"
            className="rounded-xl border border-border bg-card px-4 py-2 active:opacity-70"
          >
            <Text className="text-[13px] text-foreground">
              <Trans>返回</Trans>
            </Text>
          </Pressable>
        </View>
      </SafeAreaView>
    );
  }

  const sendable = canSendToDevice(device);

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`发送到 ${displayName}`} />

      <FileBrowser
        items={browserItems}
        scope="send"
        actions={browserActions}
        title={<Trans>已选文件</Trans>}
        contentHeader={
          <View className="gap-3 pt-2">
            <DeviceHeader device={device} runtimeState={runtimeState} />
            <AddSourceButtons disabled={sending} onPick={handlePick} />
          </View>
        }
        testID="send-file-browser"
      />

      <BottomActionBar testID="send-action-bar">
        {prepareProgress ? (
          <PrepareProgressBar progress={prepareProgress} />
        ) : (
          <View className="flex-row items-center justify-between gap-3">
            <Text className="text-[12px] text-muted-foreground">
              {selectedFiles.length > 0 ? (
                <Trans>
                  {selectedFiles.length} 个文件 · {formatBytes(totalSize)}
                </Trans>
              ) : (
                <Trans>选择要发送的内容</Trans>
              )}
            </Text>
            <View className="flex-row gap-2">
              <Pressable
                onPress={onCancel}
                accessibilityRole="button"
                disabled={sending}
                className={cn(
                  "h-10 flex-row items-center justify-center rounded-xl border border-border bg-card px-4",
                  "active:opacity-70 disabled:opacity-50",
                )}
              >
                <Text className="text-[13px] text-foreground">
                  <Trans>取消</Trans>
                </Text>
              </Pressable>
              <Pressable
                onPress={onSend}
                accessibilityRole="button"
                disabled={sending || selectedFiles.length === 0 || !sendable}
                className={cn(
                  "h-10 min-w-25 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary px-4",
                  "active:opacity-70 disabled:opacity-50",
                )}
              >
                {sending ? (
                  <ActivityIndicator
                    color={colors.primaryForeground}
                    size="small"
                  />
                ) : null}
                <Text className="text-[13px] font-semibold text-primary-foreground">
                  {sending ? <Trans>准备中</Trans> : <Trans>发送</Trans>}
                </Text>
              </Pressable>
            </View>
          </View>
        )}
      </BottomActionBar>
    </SafeAreaView>
  );
}

/* ─── 顶部 device header ─── */

function DeviceHeader({
  device,
  runtimeState,
}: {
  device: DeviceInfo;
  runtimeState: string;
}) {
  const colors = useThemeColors();
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const isOnline = device.status === "online";
  const trustLevel = resolveTrustLevel(device);

  return (
    <View className="flex-row items-center gap-3 rounded-xl bg-primary/10 p-3.5">
      <View className="size-10 items-center justify-center rounded-full bg-card">
        <Icon color={colors.foreground} size={20} />
      </View>
      <View className="flex-1 gap-0.5">
        <Text
          className="text-[14px] font-semibold text-foreground"
          numberOfLines={1}
        >
          {deviceDisplayName(device)}
        </Text>
        <Text className="text-[11px] text-muted-foreground">
          {trustLevel === "blocked" ? (
            <Trans>已阻止 · 不可发送</Trans>
          ) : isOnline ? (
            <Trans>在线 · 可接收</Trans>
          ) : runtimeState !== "running" ? (
            <Trans>节点未启动</Trans>
          ) : (
            <Trans>离线 · 等待对端上线</Trans>
          )}
        </Text>
      </View>
    </View>
  );
}

/* ─── 添加来源按钮组 ─── */

interface SourceDef {
  key: "files" | "directory" | "photos" | "videos";
  icon: LucideIcon;
  label: React.ReactNode;
}

function AddSourceButtons({
  disabled,
  onPick,
}: {
  disabled: boolean;
  onPick: (kind: SourceDef["key"]) => void;
}) {
  const sources: SourceDef[] = [
    { key: "files", icon: FileText, label: <Trans>文件</Trans> },
    { key: "directory", icon: Folder, label: <Trans>文件夹</Trans> },
    { key: "photos", icon: ImageIcon, label: <Trans>照片</Trans> },
    { key: "videos", icon: Video, label: <Trans>视频</Trans> },
  ];
  const colors = useThemeColors();
  return (
    <View className="flex-row gap-2">
      {sources.map(({ key, icon: Icon, label }) => (
        <Pressable
          key={key}
          onPress={() => onPick(key)}
          disabled={disabled}
          accessibilityRole="button"
          className="flex-1 items-center gap-1 rounded-xl border border-border bg-card py-2.5 active:opacity-70 disabled:opacity-50"
        >
          <View className="size-8 items-center justify-center rounded-full bg-primary/10">
            <Icon color={colors.primary} size={16} />
          </View>
          <Text className="text-[11px] font-medium text-foreground">
            {label}
          </Text>
        </Pressable>
      ))}
    </View>
  );
}

/* ─── 准备阶段进度条（订阅 PrepareProgress 事件） ─── */

function PrepareProgressBar({ progress }: { progress: MobilePrepareProgress }) {
  const total = Number(progress.totalBytes);
  const hashed = Number(progress.bytesHashed);
  return (
    <View className="flex-1 gap-2">
      <View className="flex-row items-center justify-between gap-3">
        <Text
          className="flex-1 text-[12px] text-muted-foreground"
          numberOfLines={1}
        >
          <Trans>
            正在计算校验和 ({progress.completedFiles.toString()}/
            {progress.totalFiles.toString()})
          </Trans>
        </Text>
        <Text className="text-[11px] text-muted-foreground">
          {formatBytes(hashed)} / {formatBytes(total)}
        </Text>
      </View>
      <ProgressBar percent={calcPercent(hashed, total)} heightClass="h-1.5" />
      {progress.currentFile ? (
        <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
          {progress.currentFile}
        </Text>
      ) : null}
    </View>
  );
}
