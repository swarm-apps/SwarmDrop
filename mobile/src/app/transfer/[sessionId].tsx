import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams } from "expo-router";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  FolderOpen,
} from "lucide-react-native";
import { Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { getMobilePaths } from "@/core/paths";
import { useThemeColors } from "@/hooks/useThemeColors";
import { useTransferStore } from "@/stores/transfer-store";

export default function TransferDetailScreen() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const { sessionId } = useLocalSearchParams<{ sessionId: string }>();
  const snapshot = useTransferStore((s) =>
    sessionId ? s.progress[sessionId] : undefined,
  );

  const total = snapshot ? Number(snapshot.totalBytes) : 0;
  const transferred = snapshot ? Number(snapshot.transferredBytes) : 0;
  const percent =
    total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : 0;
  const isOutgoing = snapshot?.direction === "send";

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`传输详情`} />
      <ScrollView contentContainerClassName="gap-4 px-5 pt-2 pb-8">
        {!snapshot ? (
          <View className="items-center gap-2 py-20">
            <Text className="text-sm text-muted-foreground">
              <Trans>会话不存在或已结束</Trans>
            </Text>
          </View>
        ) : (
          <>
            <View className="flex-row items-center gap-3 rounded-xl border border-border bg-card p-4">
              <View
                className={
                  isOutgoing
                    ? "size-11 items-center justify-center rounded-full bg-primary/10"
                    : "size-11 items-center justify-center rounded-full bg-success/10"
                }
              >
                {isOutgoing ? (
                  <ArrowUpFromLine color={colors.primary} size={20} />
                ) : (
                  <ArrowDownToLine color={colors.success} size={20} />
                )}
              </View>
              <View className="flex-1 gap-0.5">
                <Text className="text-[14px] font-semibold text-foreground">
                  {isOutgoing ? <Trans>发送中</Trans> : <Trans>接收中</Trans>}
                </Text>
                <Text className="text-[11px] text-muted-foreground">
                  {snapshot.completedFiles.toString()}/
                  {snapshot.totalFiles.toString()} <Trans>文件</Trans> ·{" "}
                  {formatBytes(transferred)} / {formatBytes(total)}
                </Text>
              </View>
              <Text className="text-base font-bold text-foreground">
                {percent}%
              </Text>
            </View>

            <View className="gap-2">
              <Text className="text-xs text-muted-foreground">
                <Trans>进度</Trans>
              </Text>
              <View className="h-2 overflow-hidden rounded-full bg-muted">
                <View
                  className="h-full bg-primary"
                  style={{ width: `${percent}%` }}
                />
              </View>
            </View>

            {!isOutgoing ? (
              <Pressable
                onPress={() => {
                  console.log(
                    "[transfer-detail] inbox uri:",
                    getMobilePaths().transfersInboxUri,
                  );
                }}
                accessibilityRole="button"
                className="flex-row items-center gap-2 rounded-xl border border-border bg-card px-3 py-3 active:opacity-70"
              >
                <FolderOpen color={colors.foreground} size={16} />
                <Text className="flex-1 text-[13px] text-foreground">
                  <Trans>查看保存位置</Trans>
                </Text>
              </Pressable>
            ) : null}
          </>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
