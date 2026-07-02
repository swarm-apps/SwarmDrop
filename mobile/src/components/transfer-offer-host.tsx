import { Trans, useLingui } from "@lingui/react/macro";
import { Directory } from "expo-file-system";
import { useRouter } from "expo-router";
import { Bot, Download, FolderOpen } from "lucide-react-native";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Dimensions,
  Platform,
  Pressable,
  ScrollView,
  View,
} from "react-native";
import { MobileTransferOrigin_Tags } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { buildTreeDataFromOffer, FileTree } from "@/components/file-tree";
import { formatBytes } from "@/components/transfer/shared";
import { TrustBadge } from "@/components/trust-badge";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Text } from "@/components/ui/text";
import { resolveTrustLevel } from "@/core/device-trust";
import { getMobileCore } from "@/core/mobile-core";
import { resolveReceiveLocation } from "@/core/paths";
import { policyActionLabel } from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export function TransferOfferHost() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const current = useTransferStore((s) => s.currentOffer);
  const dismiss = useTransferStore((s) => s.dismissOffer);
  const loadProjection = useTransferStore((s) => s.loadProjection);
  const setError = useTransferStore((s) => s.setError);
  const { devices, pairedDevicesCache } = useMobileCoreStore(
    useShallow((s) => ({
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
    })),
  );
  const [busy, setBusy] = useState<"accepting" | "rejecting" | null>(null);
  // 本次 offer 的保存目标覆盖;null 表示走全局默认接收位置。
  const [saveDir, setSaveDir] = useState<string | null>(null);

  const open = current !== null;

  // 切换到新 offer 时清掉上一次的目标覆盖。
  // biome-ignore lint/correctness/useExhaustiveDependencies: 只需在 offer id 变化时重置。
  useEffect(() => {
    setSaveDir(null);
  }, [current?.id]);

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

  const treeData = useMemo(
    () =>
      current
        ? buildTreeDataFromOffer(
            current.offer.files.map((file) => ({
              fileId: file.fileId,
              name: file.name,
              relativePath: file.relativePath || file.name,
              size: Number(file.size),
            })),
          )
        : null,
    [current],
  );

  const pickSaveDir = useCallback(async () => {
    try {
      const dir = await Directory.pickDirectoryAsync();
      try {
        dir.list();
      } catch (probeErr) {
        toast.error(t`此目录不可读`, probeErr);
        return;
      }
      setSaveDir(dir.uri);
    } catch (err) {
      toast.error(t`选择失败`, err);
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
      // 关闭 dialog 后进入详情页观察实时进度。push 而不是 replace：
      // 用户可能在任意屏幕收到 offer，按返回应回原屏，不应破坏导航栈。
      router.push({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }, [busy, current, dismiss, setError, loadProjection, router, saveDir]);

  const reject = useCallback(async () => {
    if (!current || busy !== null) return;
    setBusy("rejecting");
    try {
      await getMobileCore().rejectReceive(current.id);
    } catch (err) {
      console.warn("[transfer-offer-host] reject failed:", err);
    } finally {
      dismiss(current.id);
      setBusy(null);
    }
  }, [busy, current, dismiss]);

  if (!open || !current) return null;

  const totalLabel = formatBytes(Number(current.offer.totalSize));
  const trustLevel = pairedDevice ? resolveTrustLevel(pairedDevice) : null;
  const policyNote = offerPolicyNote(
    current.offer.policyAction,
    current.offer.policyReason,
  );
  const rejectedByPolicy = current.offer.policyAction === "reject";

  return (
    <AlertDialog open={open}>
      <AlertDialogContent
        testID="transfer-offer-dialog"
        style={[
          {
            borderRadius: 20,
            borderWidth: 0,
            width: Math.min(Dimensions.get("window").width * 0.86, 480),
          },
          Platform.OS === "android"
            ? { elevation: 24 }
            : {
                shadowColor: "#000",
                shadowOffset: { width: 0, height: 16 },
                shadowOpacity: 0.2,
                shadowRadius: 40,
              },
        ]}
        className="max-w-none gap-4 bg-card p-5 sm:max-w-none"
      >
        <View className="flex-row items-center gap-3">
          <View className="size-11 items-center justify-center rounded-full bg-primary/10">
            <Download color={colors.primary} size={22} />
          </View>
          <View className="flex-1 gap-0.5">
            <AlertDialogTitle className="text-foreground text-base font-bold">
              {rejectedByPolicy ? (
                <Trans>已拒绝文件</Trans>
              ) : (
                <Trans>收到文件</Trans>
              )}
            </AlertDialogTitle>
            <Text className="text-xs text-muted-foreground" numberOfLines={1}>
              {current.offer.deviceName} · {current.offer.files.length}{" "}
              <Trans>个文件</Trans> · {totalLabel}
            </Text>
            {current.offer.origin.tag === MobileTransferOrigin_Tags.Mcp && (
              <View className="mt-1 flex-row items-center gap-1 self-start rounded-full bg-primary/10 px-2 py-0.5">
                <Bot color={colors.primary} size={12} />
                <Text className="text-[11px] font-medium text-primary-ink">
                  {current.offer.origin.inner.client ? (
                    <Trans>
                      由 AI 代理发起（{current.offer.origin.inner.client}）
                    </Trans>
                  ) : (
                    <Trans>由 AI 代理发起</Trans>
                  )}
                </Text>
              </View>
            )}
          </View>
        </View>

        <View className="gap-2 rounded-xl bg-muted px-3.5 py-3">
          <View className="flex-row items-center justify-between gap-3">
            <Text className="text-[12px] font-semibold text-foreground">
              <Trans>设备策略</Trans>
            </Text>
            {trustLevel ? (
              <TrustBadge level={trustLevel} compact />
            ) : (
              <Text className="text-[11px] text-muted-foreground">
                <Trans>未配对</Trans>
              </Text>
            )}
          </View>
          <Text className="text-[12px] text-muted-foreground">
            {policyNote ?? <Trans>此设备需要手动确认后才会开始接收。</Trans>}
          </Text>
        </View>

        {!rejectedByPolicy ? (
          <View className="flex-row items-center justify-between gap-3 rounded-xl bg-muted px-3.5 py-3">
            <View className="min-w-0 flex-1 flex-row items-center gap-2">
              <FolderOpen color={colors.mutedForeground} size={15} />
              <View className="min-w-0 flex-1">
                <Text className="text-[11px] text-muted-foreground">
                  <Trans>保存到</Trans>
                </Text>
                <Text
                  className="text-[12px] text-foreground"
                  numberOfLines={1}
                  testID="transfer-offer-save-destination"
                >
                  {saveDir ? (
                    prettyDestination(saveDir)
                  ) : (
                    <Trans>默认接收位置</Trans>
                  )}
                </Text>
              </View>
            </View>
            <Pressable
              onPress={pickSaveDir}
              accessibilityRole="button"
              accessibilityLabel={t`更改保存位置`}
              testID="transfer-offer-change-save-dir"
              className="min-h-11 items-center justify-center rounded-lg border border-border px-3.5 active:opacity-70"
            >
              <Text className="text-[12px] font-semibold text-foreground">
                <Trans>更改</Trans>
              </Text>
            </Pressable>
          </View>
        ) : null}

        <ScrollView
          className="max-h-[220px] rounded-xl bg-muted"
          contentContainerClassName="p-3"
          testID="transfer-offer-file-list"
        >
          {treeData ? (
            <FileTree
              mode="select"
              dataLoader={treeData.dataLoader}
              rootChildren={treeData.rootChildren}
              totalCount={current.offer.files.length}
              totalSize={Number(current.offer.totalSize)}
              progress={null}
              showHeader={false}
            />
          ) : null}
        </ScrollView>

        <View className="gap-2.5">
          {rejectedByPolicy ? (
            <Pressable
              onPress={() => dismiss(current.id)}
              accessibilityRole="button"
              accessibilityLabel={t`知道了`}
              testID="transfer-offer-dismiss-button"
              className="h-12 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
            >
              <Text className="text-base font-medium text-foreground">
                <Trans>知道了</Trans>
              </Text>
            </Pressable>
          ) : (
            <>
              <Pressable
                onPress={accept}
                disabled={busy !== null}
                accessibilityRole="button"
                accessibilityLabel={t`接收`}
                testID="transfer-offer-accept-button"
                className="h-12 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
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
              <Pressable
                onPress={reject}
                disabled={busy !== null}
                accessibilityRole="button"
                accessibilityLabel={t`拒绝`}
                testID="transfer-offer-reject-button"
                className="h-12 items-center justify-center rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
              >
                <Text className="text-base font-medium text-foreground">
                  <Trans>拒绝</Trans>
                </Text>
              </Pressable>
            </>
          )}
        </View>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function offerPolicyNote(
  action?: string | null,
  reason?: string | null,
): string | null {
  if (reason) {
    return action ? `${policyActionLabel(action)}：${reason}` : reason;
  }
  return action ? policyActionLabel(action) : null;
}

/** 把 file:// / content:// URI 截成更短的显示串:取最后一段路径。 */
function prettyDestination(uri: string): string {
  try {
    const decoded = decodeURIComponent(uri.replace(/\/$/, ""));
    const segments = decoded.split("/");
    const last = segments[segments.length - 1];
    return last && last.length > 0 ? last : decoded;
  } catch {
    return uri;
  }
}
