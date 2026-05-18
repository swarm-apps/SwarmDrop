import { Trans, useLingui } from "@lingui/react/macro";
import { Download, File as FileIcon } from "lucide-react-native";
import { useCallback, useState } from "react";
import {
  ActivityIndicator,
  Dimensions,
  Platform,
  Pressable,
  ScrollView,
  View,
} from "react-native";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { getMobilePaths } from "@/core/paths";
import { useThemeColors } from "@/hooks/useThemeColors";
import { useTransferStore } from "@/stores/transfer-store";

export function TransferOfferHost() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const current = useTransferStore((s) => s.currentOffer);
  const dismiss = useTransferStore((s) => s.dismissOffer);
  const registerSession = useTransferStore((s) => s.registerSession);
  const setError = useTransferStore((s) => s.setError);
  const [busy, setBusy] = useState<"accepting" | "rejecting" | null>(null);

  const open = current !== null;

  const accept = useCallback(async () => {
    if (!current || busy !== null) return;
    setBusy("accepting");
    try {
      const { transfersInboxUri } = getMobilePaths();
      await getMobileCore().acceptReceive(current.id, transfersInboxUri);
      registerSession(current.id);
      dismiss(current.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }, [busy, current, dismiss, setError, registerSession]);

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
  const previewFiles = current.offer.files.slice(0, 5);
  const remainingCount = current.offer.files.length - previewFiles.length;

  return (
    <AlertDialog open={open}>
      <AlertDialogContent
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
              <Trans>收到文件</Trans>
            </AlertDialogTitle>
            <Text className="text-xs text-muted-foreground" numberOfLines={1}>
              {current.offer.deviceName} · {current.offer.files.length}{" "}
              <Trans>个文件</Trans> · {totalLabel}
            </Text>
          </View>
        </View>

        <ScrollView
          className="max-h-[220px] rounded-xl bg-muted"
          contentContainerClassName="p-3 gap-2"
        >
          {previewFiles.map((f) => (
            <View key={f.fileId} className="flex-row items-center gap-2.5">
              <FileIcon color={colors.mutedForeground} size={14} />
              <Text
                className="flex-1 text-[13px] text-foreground"
                numberOfLines={1}
              >
                {f.name}
              </Text>
              <Text className="text-[11px] text-muted-foreground">
                {formatBytes(Number(f.size))}
              </Text>
            </View>
          ))}
          {remainingCount > 0 ? (
            <Text className="text-center text-[11px] italic text-muted-foreground">
              <Trans>...还有 {remainingCount} 个文件</Trans>
            </Text>
          ) : null}
        </ScrollView>

        <View className="gap-2.5">
          <Pressable
            onPress={accept}
            disabled={busy !== null}
            accessibilityRole="button"
            accessibilityLabel={t`接收`}
            className="h-12 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-80 disabled:opacity-50"
          >
            {busy === "accepting" ? (
              <ActivityIndicator color={colors.background} />
            ) : (
              <Download color={colors.background} size={16} />
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
            className="h-12 items-center justify-center rounded-xl border border-border bg-card active:opacity-80 disabled:opacity-50"
          >
            <Text className="text-base font-medium text-foreground">
              <Trans>拒绝</Trans>
            </Text>
          </Pressable>
        </View>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
