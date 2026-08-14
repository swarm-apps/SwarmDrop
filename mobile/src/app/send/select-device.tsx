/**
 * 发送准备页 —— 与桌面端 `/_app/send` 对齐。
 *
 * 入口：(main) 点击在线设备 → push 到这里（携带 peerId）。
 * 流程：
 *   1. 顶部 device header（不再让用户选设备，已经在主屏选过）
 *   2. 文件/文本紧凑分段控件：文件默认但不预设入口语义
 *   3. 文件模式以「添加文件 / 添加文件夹 / 照片 / 视频」累加；文本模式提供编辑器与剪贴板
 *   4. 底部「取消 / 发送」操作栏；文件发送过程显示 prepareSend 进度条
 *   5. 文件发送成功 → router.replace 到 `/transfer/[sessionId]` 看实时进度
 */

import { Trans, useLingui } from "@lingui/react/macro";
import {
  formatTextDeliveryKiB,
  isTextDeliveryRetryable,
  isTextDeliveryWithinLimit,
  organizedDeviceName,
  TEXT_DELIVERY_MAX_BYTES,
  type TextDeliveryStatus,
  utf8ByteLength,
} from "@swarmdrop/shared-view";
import { useLocalSearchParams, useRouter } from "expo-router";
import {
  Clipboard as ClipboardIcon,
  FileText,
  Folder,
  Image as ImageIcon,
  type LucideIcon,
  Video,
} from "lucide-react-native";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  ActivityIndicator,
  Alert,
  Pressable,
  ScrollView,
  TextInput,
  View,
} from "react-native";
import {
  type MobileDevice as DeviceInfo,
  type MobileTextDeliveryRecord,
  MobileTextDeliveryStatus,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  FileBrowser,
  type FileBrowserActions,
  fromSelectedFiles,
} from "@/components/file-browser";
import {
  AppScreen,
  BottomActionBar,
  Surface,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { PrepareProgressBar } from "@/components/transfer/prepare-progress-bar";
import { formatBytes, formatRelativeTime } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { canSendToDevice, resolveTrustLevel } from "@/core/device-trust";
import {
  pickFromMediaLibrary,
  pickTransferDirectory,
  pickTransferFiles,
} from "@/core/file-access";
import { getMobileCore } from "@/core/mobile-core";
import { useThemeColors } from "@/hooks/useThemeColors";
import { clipboard } from "@/lib/clipboard";
import { devicePlatformIcon } from "@/lib/device-platform";
import { getErrorMessage } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import {
  useActivePrepareProgress,
  useTransferStore,
} from "@/stores/transfer-store";

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
  const deviceOrganization = usePreferencesStore((s) => s.deviceOrganization);

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

  const displayName = device
    ? organizedDeviceName(device, deviceOrganization)
    : "";

  // ── 准备阶段进度 ───────────────────────────────────────────
  // 住在 store 而不是本页 useState：准备大目录可以是分钟级，用户切走再回来得看得到它。
  const [sending, setSending] = useState(false);
  const [contentMode, setContentMode] = useState<"files" | "text">("files");
  const [textBody, setTextBody] = useState("");
  const [outbox, setOutbox] = useState<MobileTextDeliveryRecord[]>([]);
  const prepareProgress = useActivePrepareProgress();

  const browserItems = useMemo(
    () => fromSelectedFiles(selectedFiles),
    [selectedFiles],
  );
  const browserActions = useMemo<FileBrowserActions>(
    () => ({
      removeItem: (item) => {
        // 共享模型的 `sourceId` 是 `string | number`（收件箱那一路用行主键）。
        // 发送侧存的一律是 `file://` 来源串，转一次窄回 string。
        if (item.sourceId) removeSelectedBySourceId(String(item.sourceId));
      },
      removeDirectory: removeSelectedDirectory,
    }),
    [removeSelectedBySourceId, removeSelectedDirectory],
  );

  const totalSize = selectedFiles.reduce((s, f) => s + f.size, 0n);

  const refreshTextOutbox = useCallback(async () => {
    if (!device) return;
    try {
      setOutbox(await getMobileCore().listTextOutbox(device.peerId));
    } catch (error) {
      toast.error(t`读取发送记录失败`, error);
    }
  }, [device, t]);

  useEffect(() => {
    if (contentMode === "text") void refreshTextOutbox();
  }, [contentMode, refreshTextOutbox]);

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
        toast.error(t`选择失败`, getErrorMessage(err));
      }
    },
    [appendFiles, t],
  );

  // ── 发送 ───────────────────────────────────────────────────
  const onSend = useCallback(async () => {
    if (
      !device ||
      sending ||
      (contentMode === "files" && selectedFiles.length === 0) ||
      (contentMode === "text" && !isTextDeliveryWithinLimit(textBody))
    ) {
      return;
    }
    setSending(true);
    try {
      if (contentMode === "text") {
        await getMobileCore().sendTextDelivery(
          device.peerId,
          displayName,
          textBody,
        );
        setTextBody("");
        toast.success(t`文本已发送`);
        await refreshTextOutbox();
        router.back();
        return;
      }
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
      toast.error(t`发送失败`, panicDetail ?? getErrorMessage(err));
    } finally {
      setSending(false);
    }
  }, [
    device,
    sending,
    contentMode,
    textBody,
    selectedFiles,
    displayName,
    startSend,
    clearSelectedFiles,
    refreshTextOutbox,
    router,
    t,
  ]);

  const onCancel = useCallback(() => {
    if (selectedFiles.length > 0 || textBody.length > 0) {
      Alert.alert(
        t`放弃发送内容？`,
        contentMode === "text"
          ? t`输入的文本将被清空。`
          : t`已选的文件将被清空。`,
        [
          { text: t`继续选择`, style: "cancel" },
          {
            text: t`放弃`,
            style: "destructive",
            onPress: () => {
              clearSelectedFiles();
              router.back();
            },
          },
        ],
      );
    } else {
      router.back();
    }
  }, [
    selectedFiles.length,
    textBody.length,
    contentMode,
    clearSelectedFiles,
    router,
    t,
  ]);

  // ── 渲染 ───────────────────────────────────────────────────
  if (!device) {
    return (
      <AppScreen header={<SettingsHeader title={t`发送`} />} bare>
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
      </AppScreen>
    );
  }

  const sendable = canSendToDevice(device);

  return (
    <AppScreen
      header={<SettingsHeader title={t`发送到 ${displayName}`} />}
      bare
    >
      {contentMode === "files" ? (
        <FileBrowser
          items={browserItems}
          scope="send"
          actions={browserActions}
          title={<Trans>已选文件</Trans>}
          contentHeader={
            <View className="gap-3 pt-2">
              <DeviceHeader
                device={device}
                displayName={displayName}
                runtimeState={runtimeState}
              />
              <ContentModeSwitch
                value={contentMode}
                onChange={setContentMode}
                disabled={sending}
              />
              <AddSourceButtons disabled={sending} onPick={handlePick} />
            </View>
          }
          testID="send-file-browser"
        />
      ) : (
        <ScrollView
          className="flex-1"
          contentContainerClassName="gap-3 px-5 pt-3"
          keyboardShouldPersistTaps="handled"
        >
          <DeviceHeader
            device={device}
            displayName={displayName}
            runtimeState={runtimeState}
          />
          <ContentModeSwitch
            value={contentMode}
            onChange={setContentMode}
            disabled={sending}
          />
          <Surface className="gap-3 p-4">
            <View className="flex-row items-center justify-between gap-3">
              <Text className="text-[14px] font-semibold text-foreground">
                <Trans>文本内容</Trans>
              </Text>
              <View className="flex-row gap-2">
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={t`清空文本`}
                  disabled={sending || textBody.length === 0}
                  onPress={() => setTextBody("")}
                  className="min-h-11 justify-center rounded-lg border border-border px-3 py-1.5 active:opacity-70 disabled:opacity-50"
                >
                  <Text className="text-[12px] font-semibold text-foreground">
                    <Trans>清空</Trans>
                  </Text>
                </Pressable>
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={t`从剪贴板粘贴`}
                  disabled={sending}
                  testID="send-text-paste-button"
                  onPress={() => {
                    void clipboard
                      .readText()
                      .then((value) => {
                        if (value.length === 0) {
                          toast.error(t`剪贴板中没有可用文本`);
                          return;
                        }
                        setTextBody(value);
                      })
                      .catch((error) => toast.error(t`读取剪贴板失败`, error));
                  }}
                  className="min-h-11 justify-center rounded-lg border border-border px-3 py-1.5 active:opacity-70 disabled:opacity-50"
                >
                  <Text className="text-[12px] font-semibold text-foreground">
                    <Trans>粘贴</Trans>
                  </Text>
                </Pressable>
              </View>
            </View>
            <TextInput
              multiline
              value={textBody}
              onChangeText={setTextBody}
              editable={!sending}
              accessibilityLabel={t`要发送的文本`}
              testID="send-text-editor"
              placeholder={t`输入或粘贴文本`}
              placeholderTextColor={colors.mutedForeground}
              textAlignVertical="top"
              className="min-h-48 rounded-xl border border-border bg-muted/30 p-3 text-[15px] leading-6 text-foreground"
            />
            {textBody.length > 0 && !isTextDeliveryWithinLimit(textBody) ? (
              <Text className="text-[12px] text-destructive-ink">
                <Trans>文本超过 64 KiB，请缩短后发送。</Trans>
              </Text>
            ) : null}
            {outbox.length > 0 ? (
              <View className="gap-2 border-t border-border pt-3">
                <Text className="text-[13px] font-semibold text-muted-foreground">
                  <Trans>最近发送</Trans>
                </Text>
                {outbox.slice(0, 5).map((record) => (
                  <View
                    key={record.deliveryId}
                    className="gap-2 rounded-xl border border-border bg-muted/30 p-3"
                  >
                    <View className="flex-row items-center justify-between gap-2">
                      <Text
                        className="flex-1 text-[13px] text-foreground"
                        numberOfLines={2}
                      >
                        {record.body}
                      </Text>
                      <Text className="text-[11px] text-muted-foreground">
                        {mobileTextStatus(record.status)}
                      </Text>
                    </View>
                    <Text className="text-[11px] text-muted-foreground">
                      <Trans>更新于</Trans>{" "}
                      {formatRelativeTime(record.updatedAt)}
                    </Text>
                    <View className="flex-row justify-end gap-2">
                      <Pressable
                        accessibilityRole="button"
                        onPress={() => setTextBody(record.body)}
                        className="rounded-lg border border-border px-3 py-1.5 active:opacity-70"
                      >
                        <Text className="text-[12px] font-semibold text-foreground">
                          <Trans>编辑后重发</Trans>
                        </Text>
                      </Pressable>
                      {mobileTextRetryable(record.status) ? (
                        <Pressable
                          accessibilityRole="button"
                          testID={`send-text-retry-${record.deliveryId}`}
                          onPress={() =>
                            void getMobileCore()
                              .retryTextDelivery(record.deliveryId)
                              .then(refreshTextOutbox)
                              .catch((error) => toast.error(t`重试失败`, error))
                          }
                          className="rounded-lg border border-border px-3 py-1.5 active:opacity-70"
                        >
                          <Text className="text-[12px] font-semibold text-foreground">
                            <Trans>重试</Trans>
                          </Text>
                        </Pressable>
                      ) : null}
                      <Pressable
                        accessibilityRole="button"
                        onPress={() =>
                          void getMobileCore()
                            .deleteTextOutboxRecord(record.deliveryId)
                            .then(refreshTextOutbox)
                            .catch((error) => toast.error(t`删除失败`, error))
                        }
                        className="rounded-lg border border-border px-3 py-1.5 active:opacity-70"
                      >
                        <Text className="text-[12px] font-semibold text-foreground">
                          <Trans>删除</Trans>
                        </Text>
                      </Pressable>
                    </View>
                  </View>
                ))}
              </View>
            ) : null}
          </Surface>
        </ScrollView>
      )}

      {/* 进度条**叠在按钮行之上**，不替换它（与桌面同形）。替换式的写法让准备期间这一屏
          一个可交互元素都不剩：连取消都没了，而 `activePrepare` 万一没被收干净（迟到的
          收尾事件、中途失败的批次）就再也点不动发送，只能重启应用。 */}
      <BottomActionBar testID="send-action-bar">
        <View className="flex-1 gap-2">
          {prepareProgress ? (
            <PrepareProgressBar progress={prepareProgress} />
          ) : null}
          <View className="flex-row items-center justify-between gap-3">
            <Text
              className="flex-1 text-[13px] text-muted-foreground"
              numberOfLines={1}
            >
              {contentMode === "text" ? (
                <>
                  {formatTextDeliveryKiB(utf8ByteLength(textBody))} /{" "}
                  {formatTextDeliveryKiB(TEXT_DELIVERY_MAX_BYTES)}
                </>
              ) : selectedFiles.length > 0 ? (
                <Trans>
                  {selectedFiles.length} 个文件 · {formatBytes(totalSize)}
                </Trans>
              ) : (
                <Trans>选择要发送的内容</Trans>
              )}
            </Text>
            <View className="shrink-0 flex-row gap-2">
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
                testID={
                  contentMode === "text"
                    ? "send-text-action"
                    : "send-files-action"
                }
                disabled={
                  sending ||
                  !sendable ||
                  (contentMode === "files"
                    ? selectedFiles.length === 0
                    : !isTextDeliveryWithinLimit(textBody))
                }
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
        </View>
      </BottomActionBar>
    </AppScreen>
  );
}

function mobileTextRetryable(status: MobileTextDeliveryStatus) {
  return isTextDeliveryRetryable(mobileTextDeliveryStatus(status));
}

function mobileTextDeliveryStatus(
  status: MobileTextDeliveryStatus,
): TextDeliveryStatus {
  switch (status) {
    case MobileTextDeliveryStatus.Sending:
      return "sending";
    case MobileTextDeliveryStatus.WaitingConfirmation:
      return "waiting_confirmation";
    case MobileTextDeliveryStatus.Delivered:
      return "delivered";
    case MobileTextDeliveryStatus.Rejected:
      return "rejected";
    case MobileTextDeliveryStatus.Retryable:
      return "retryable";
    case MobileTextDeliveryStatus.Expired:
      return "expired";
    case MobileTextDeliveryStatus.Cancelled:
      return "cancelled";
  }
}

function mobileTextStatus(status: MobileTextDeliveryStatus): ReactNode {
  switch (status) {
    case MobileTextDeliveryStatus.Sending:
      return <Trans>发送中</Trans>;
    case MobileTextDeliveryStatus.WaitingConfirmation:
      return <Trans>等待确认</Trans>;
    case MobileTextDeliveryStatus.Delivered:
      return <Trans>已送达</Trans>;
    case MobileTextDeliveryStatus.Rejected:
      return <Trans>已拒绝</Trans>;
    case MobileTextDeliveryStatus.Retryable:
      return <Trans>可重试，送达状态未知</Trans>;
    case MobileTextDeliveryStatus.Expired:
      return <Trans>已过期</Trans>;
    case MobileTextDeliveryStatus.Cancelled:
      return <Trans>已取消</Trans>;
  }
}

function ContentModeSwitch({
  value,
  onChange,
  disabled,
}: {
  value: "files" | "text";
  onChange: (value: "files" | "text") => void;
  disabled: boolean;
}) {
  const options = [
    { value: "files" as const, label: <Trans>文件</Trans>, icon: FileText },
    { value: "text" as const, label: <Trans>文本</Trans>, icon: ClipboardIcon },
  ];
  const colors = useThemeColors();
  return (
    <View className="self-start flex-row rounded-xl bg-muted p-1">
      {options.map(({ value: option, label, icon: Icon }) => {
        const active = option === value;
        return (
          <Pressable
            key={option}
            accessibilityRole="button"
            accessibilityState={{ selected: active }}
            disabled={disabled}
            onPress={() => onChange(option)}
            testID={`send-content-mode-${option}`}
            className={cn(
              "h-11 w-28 flex-row items-center justify-center gap-1.5 rounded-lg",
              active ? "bg-card" : "",
              "disabled:opacity-50",
            )}
          >
            <Icon
              color={active ? colors.foreground : colors.mutedForeground}
              size={15}
            />
            <Text className="text-[13px] font-semibold text-foreground">
              {label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

/* ─── 顶部 device header ─── */

function DeviceHeader({
  device,
  displayName,
  runtimeState,
}: {
  device: DeviceInfo;
  displayName: string;
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
          {displayName}
        </Text>
        <Text className="text-[12px] text-muted-foreground">
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
          <Text className="text-[12px] font-medium text-foreground">
            {label}
          </Text>
        </Pressable>
      ))}
    </View>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
