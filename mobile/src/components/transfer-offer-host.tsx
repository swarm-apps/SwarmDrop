import { Trans, useLingui } from "@lingui/react/macro";
import { Directory } from "expo-file-system";
import { useRouter } from "expo-router";
import { Bot, Download, FolderOpen } from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Platform,
  Pressable,
  useWindowDimensions,
  View,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import {
  type MobileDevice,
  MobileTransferOrigin_Tags,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { EncryptionNote } from "@/components/encryption-note";
import {
  FileBrowser,
  type FileBrowserItem,
  type FileBrowserListContext,
  fromOfferFiles,
} from "@/components/file-browser";
import { formatBytes } from "@/components/transfer/shared";
import { TrustBadge } from "@/components/trust-badge";
import { AlertDialog, AlertDialogContent } from "@/components/ui/alert-dialog";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { Text } from "@/components/ui/text";
import { resolveTrustLevel } from "@/core/device-trust";
import { getMobileCore } from "@/core/mobile-core";
import { resolveReceiveLocation } from "@/core/paths";
import {
  policyNoteOf,
  type TransferOfferQueueItem,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { lastPathSegment } from "@/lib/utils";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export function TransferOfferHost() {
  const { t } = useLingui();
  const router = useRouter();
  const { width } = useWindowDimensions();
  const sheetRef = useRef<AppBottomSheetRef>(null);
  const current = useTransferStore((state) => state.currentOffer);
  const dismiss = useTransferStore((state) => state.dismissOffer);
  const loadProjection = useTransferStore((state) => state.loadProjection);
  const setError = useTransferStore((state) => state.setError);
  const { devices, pairedDevicesCache } = useMobileCoreStore(
    useShallow((state) => ({
      devices: state.devices,
      pairedDevicesCache: state.pairedDevicesCache,
    })),
  );
  const [busy, setBusy] = useState<"accepting" | "rejecting" | null>(null);
  const [saveDir, setSaveDir] = useState<string | null>(null);
  const isTablet = width >= 768;

  useEffect(() => {
    setSaveDir(null);
    setBusy(null);
    if (current && !isTablet) sheetRef.current?.present();
    else sheetRef.current?.dismiss();
  }, [current, isTablet]);

  const pairedDevice = useMemo(() => {
    if (!current) return null;
    return (
      devices.find((device) => device.peerId === current.offer.peerId) ??
      summariesToOfflineDevices(pairedDevicesCache).find(
        (device) => device.peerId === current.offer.peerId,
      ) ??
      null
    );
  }, [current, devices, pairedDevicesCache]);

  const items = useMemo(
    () => (current ? fromOfferFiles(current.id, current.offer.files) : []),
    [current],
  );

  const pickSaveDir = useCallback(async () => {
    try {
      const directory = await Directory.pickDirectoryAsync();
      try {
        directory.list();
      } catch (probeError) {
        toast.error(t`此目录不可读`, probeError);
        return;
      }
      setSaveDir(directory.uri);
    } catch (error) {
      toast.error(t`选择失败`, error);
    }
  }, [t]);

  const accept = useCallback(async () => {
    if (!current || busy !== null) return;
    setBusy("accepting");
    try {
      await getMobileCore().acceptReceive(
        current.id,
        saveDir ?? resolveReceiveLocation(),
      );
      const sessionId = current.id;
      await loadProjection(sessionId);
      dismiss(sessionId);
      router.push({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
      });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  }, [busy, current, dismiss, loadProjection, router, saveDir, setError]);

  const reject = useCallback(async () => {
    if (!current || busy !== null) return;
    setBusy("rejecting");
    try {
      await getMobileCore().rejectReceive(current.id);
    } catch (error) {
      console.warn("[transfer-offer-host] reject failed:", error);
      toast.error(t`拒绝接收失败`, error);
    } finally {
      dismiss(current.id);
      setBusy(null);
    }
  }, [busy, current, dismiss, t]);

  if (!current) return null;

  const contentProps: Omit<OfferContentProps, "listContext"> = {
    current,
    pairedDevice,
    items,
    saveDir,
    busy,
    onPickSaveDir: pickSaveDir,
    onAccept: accept,
    onReject: reject,
    onDismiss: () => dismiss(current.id),
  };

  if (!isTablet) {
    return (
      <AppBottomSheet
        ref={sheetRef}
        virtualized
        snapPoints={["92%"]}
        enablePanDownToClose={false}
        contentTestID="transfer-offer-dialog"
      >
        <OfferContent {...contentProps} listContext="bottom-sheet" />
      </AppBottomSheet>
    );
  }

  return (
    <AlertDialog open>
      <AlertDialogContent
        testID="transfer-offer-dialog"
        style={[
          { borderRadius: 20, borderWidth: 0, height: "82%", width: 680 },
          Platform.OS === "android"
            ? { elevation: 24 }
            : {
                shadowColor: "#000",
                shadowOffset: { width: 0, height: 16 },
                shadowOpacity: 0.2,
                shadowRadius: 40,
              },
        ]}
        className="max-w-none gap-0 bg-card p-0 sm:max-w-none"
      >
        <OfferContent {...contentProps} listContext="screen" />
      </AlertDialogContent>
    </AlertDialog>
  );
}

interface OfferContentProps {
  current: TransferOfferQueueItem;
  pairedDevice: MobileDevice | null;
  items: FileBrowserItem[];
  saveDir: string | null;
  busy: "accepting" | "rejecting" | null;
  listContext: FileBrowserListContext;
  onPickSaveDir: () => void;
  onAccept: () => void;
  onReject: () => void;
  onDismiss: () => void;
}

function OfferContent({
  current,
  pairedDevice,
  items,
  saveDir,
  busy,
  listContext,
  onPickSaveDir,
  onAccept,
  onReject,
  onDismiss,
}: OfferContentProps) {
  const rejectedByPolicy = current.offer.policyAction === "reject";

  return (
    <View className="flex-1">
      <OfferHeader
        current={current}
        pairedDevice={pairedDevice}
        fileCount={items.length}
        saveDir={saveDir}
        onPickSaveDir={onPickSaveDir}
      />
      <FileBrowser
        items={items}
        scope="transfer"
        listContext={listContext}
        resetKey={current.id}
        testID="transfer-offer-file-browser"
        title={<Trans>文件</Trans>}
        contentFooter={
          !rejectedByPolicy ? (
            <EncryptionNote className="justify-center py-4">
              <Trans>全程端到端加密，路上没人能看到内容</Trans>
            </EncryptionNote>
          ) : null
        }
      />
      <OfferActions
        rejectedByPolicy={rejectedByPolicy}
        busy={busy}
        safeArea={listContext === "bottom-sheet"}
        onAccept={onAccept}
        onReject={onReject}
        onDismiss={onDismiss}
      />
    </View>
  );
}

function OfferHeader({
  current,
  pairedDevice,
  fileCount,
  saveDir,
  onPickSaveDir,
}: Pick<
  OfferContentProps,
  "current" | "pairedDevice" | "saveDir" | "onPickSaveDir"
> & { fileCount: number }) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const rejectedByPolicy = current.offer.policyAction === "reject";
  const trustLevel = pairedDevice ? resolveTrustLevel(pairedDevice) : null;
  const policyNote = policyNoteOf(
    current.offer.policyAction,
    current.offer.policyReason,
  );

  return (
    <View className="gap-4 px-5 pt-4">
      <View className="flex-row items-center gap-3">
        <View className="size-11 items-center justify-center rounded-full bg-primary/10">
          <Download color={colors.primary} size={22} />
        </View>
        <View className="min-w-0 flex-1 gap-0.5">
          <Text className="text-base font-bold text-foreground">
            {rejectedByPolicy ? (
              <Trans>已拒绝文件</Trans>
            ) : (
              <Trans>收到文件</Trans>
            )}
          </Text>
          <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
            {current.offer.deviceName} · {fileCount} <Trans>个文件</Trans> ·{" "}
            {formatBytes(current.offer.totalSize)}
          </Text>
          {current.offer.origin.tag === MobileTransferOrigin_Tags.Mcp ? (
            <View className="mt-1 flex-row items-center gap-1 self-start rounded-full bg-primary/10 px-2 py-0.5">
              <Bot color={colors.primary} size={12} />
              <Text className="text-[12px] font-medium text-primary-ink">
                {current.offer.origin.inner.client ? (
                  <Trans>
                    由 AI 代理发起（{current.offer.origin.inner.client}）
                  </Trans>
                ) : (
                  <Trans>由 AI 代理发起</Trans>
                )}
              </Text>
            </View>
          ) : null}
        </View>
      </View>

      <View className="gap-2 rounded-xl bg-muted px-3.5 py-3">
        <View className="flex-row items-center justify-between gap-3">
          <Text className="text-[13px] font-semibold text-foreground">
            <Trans>设备策略</Trans>
          </Text>
          {trustLevel ? (
            <TrustBadge level={trustLevel} compact />
          ) : (
            <Text className="text-[12px] text-muted-foreground">
              <Trans>未配对</Trans>
            </Text>
          )}
        </View>
        <Text className="text-[13px] text-muted-foreground">
          {policyNote ?? <Trans>此设备需要手动确认后才会开始接收。</Trans>}
        </Text>
      </View>

      {!rejectedByPolicy ? (
        <View className="flex-row items-center justify-between gap-3 rounded-xl bg-muted px-3.5 py-3">
          <View className="min-w-0 flex-1 flex-row items-center gap-2">
            <FolderOpen color={colors.mutedForeground} size={15} />
            <View className="min-w-0 flex-1">
              <Text className="text-[12px] text-muted-foreground">
                <Trans>保存到</Trans>
              </Text>
              <Text
                className="text-[13px] text-foreground"
                numberOfLines={1}
                testID="transfer-offer-save-destination"
              >
                {saveDir ? (
                  lastPathSegment(saveDir)
                ) : (
                  <Trans>默认接收位置</Trans>
                )}
              </Text>
            </View>
          </View>
          <Pressable
            onPress={onPickSaveDir}
            accessibilityRole="button"
            accessibilityLabel={t`更改保存位置`}
            testID="transfer-offer-change-save-dir"
            className="min-h-11 items-center justify-center rounded-lg border border-border px-3.5 active:opacity-70"
          >
            <Text className="text-[13px] font-semibold text-foreground">
              <Trans>更改</Trans>
            </Text>
          </Pressable>
        </View>
      ) : null}
    </View>
  );
}

function OfferActions({
  rejectedByPolicy,
  busy,
  safeArea,
  onAccept,
  onReject,
  onDismiss,
}: Pick<OfferContentProps, "busy" | "onAccept" | "onReject" | "onDismiss"> & {
  rejectedByPolicy: boolean;
  safeArea: boolean;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const insets = useSafeAreaInsets();

  return (
    <View
      className="flex-row gap-2.5 border-t border-border bg-card px-5 pt-3"
      style={{ paddingBottom: safeArea ? Math.max(insets.bottom, 20) : 20 }}
    >
      {rejectedByPolicy ? (
        <Pressable
          onPress={onDismiss}
          accessibilityRole="button"
          accessibilityLabel={t`知道了`}
          testID="transfer-offer-dismiss-button"
          className="h-12 flex-1 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
        >
          <Text className="text-base font-medium text-foreground">
            <Trans>知道了</Trans>
          </Text>
        </Pressable>
      ) : (
        <>
          <Pressable
            onPress={onReject}
            disabled={busy !== null}
            accessibilityRole="button"
            accessibilityLabel={t`拒绝`}
            testID="transfer-offer-reject-button"
            className="h-12 flex-1 items-center justify-center rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
          >
            <Text className="text-base font-medium text-foreground">
              <Trans>拒绝</Trans>
            </Text>
          </Pressable>
          <Pressable
            onPress={onAccept}
            disabled={busy !== null}
            accessibilityRole="button"
            accessibilityLabel={t`接收`}
            testID="transfer-offer-accept-button"
            className="h-12 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
          >
            {busy === "accepting" ? (
              <ActivityIndicator color={colors.primaryForeground} />
            ) : (
              <Download color={colors.primaryForeground} size={16} />
            )}
            <Text className="text-base font-semibold text-primary-foreground">
              {busy === "accepting" ? (
                <Trans>接收中...</Trans>
              ) : (
                <Trans>接收</Trans>
              )}
            </Text>
          </Pressable>
        </>
      )}
    </View>
  );
}
