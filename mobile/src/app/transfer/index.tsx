import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { ArrowLeftRight } from "lucide-react-native";
import { useMemo } from "react";
import { ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { RecentTransferRow } from "@/components/recent-transfer-row";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { useTransferStore } from "@/stores/transfer-store";

export default function TransferHistoryScreen() {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();
  const activeSessionIds = useTransferStore((s) => s.activeSessionIds);
  const progress = useTransferStore((s) => s.progress);

  const activeSnapshots = useMemo(
    () =>
      Array.from(activeSessionIds)
        .map((sid) => progress[sid])
        .filter((p): p is NonNullable<typeof p> => p !== undefined),
    [activeSessionIds, progress],
  );

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`传输历史`} />
      <ScrollView contentContainerClassName="flex-grow gap-3 px-5 pt-2 pb-8">
        {activeSnapshots.length === 0 ? (
          <View className="flex-1 items-center justify-center gap-3 py-20">
            <View className="size-14 items-center justify-center rounded-full bg-muted">
              <ArrowLeftRight color={colors.mutedForeground} size={28} />
            </View>
            <Text className="text-sm font-medium text-foreground">
              <Trans>暂无传输记录</Trans>
            </Text>
            <Text className="text-xs text-muted-foreground">
              <Trans>在主屏选择已配对设备开始传输</Trans>
            </Text>
          </View>
        ) : (
          <View className="gap-2">
            <Text className="text-sm font-semibold text-foreground">
              <Trans>活跃传输</Trans>
            </Text>
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
        )}
      </ScrollView>
    </SafeAreaView>
  );
}
