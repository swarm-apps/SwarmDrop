import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import { ArrowLeftRight, Trash2 } from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { Alert, Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { MobileSessionStatus } from "react-native-swarmdrop-core";
import { HistoryTransferRow } from "@/components/history-transfer-row";
import { RecentTransferRow } from "@/components/recent-transfer-row";
import { SettingsHeader } from "@/components/settings-header";
import { StatusLabel } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { useTransferStore } from "@/stores/transfer-store";

type StatusFilter = MobileSessionStatus | "all";

const FILTER_OPTIONS: StatusFilter[] = [
  "all",
  MobileSessionStatus.Completed,
  MobileSessionStatus.Failed,
  MobileSessionStatus.Paused,
  MobileSessionStatus.Cancelled,
];

export default function TransferHistoryScreen() {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();

  const sessions = useTransferStore((s) => s.sessions);
  const dbHistory = useTransferStore((s) => s.dbHistory);
  const loadHistory = useTransferStore((s) => s.loadHistory);
  const clearAllHistory = useTransferStore((s) => s.clearAllHistory);

  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");

  // 进入页面时刷新一次；从其他页面 focus 回来时也刷新（用户可能刚完成一笔传输）
  useFocusEffect(
    useCallback(() => {
      void loadHistory();
    }, [loadHistory]),
  );

  const activeSnapshots = useMemo(
    () =>
      Object.values(sessions)
        .sort((a, b) => b.startedAt - a.startedAt)
        .map((s) => s.progress)
        .filter((p): p is NonNullable<typeof p> => p !== null),
    [sessions],
  );

  const filteredHistory = useMemo(
    () =>
      statusFilter === "all"
        ? dbHistory
        : dbHistory.filter((item) => item.status === statusFilter),
    [dbHistory, statusFilter],
  );

  const hasContent = activeSnapshots.length > 0 || filteredHistory.length > 0;

  const handleClear = useCallback(() => {
    Alert.alert(
      t`清空传输历史`,
      t`这将删除全部已结束的传输记录，正在进行的传输不受影响。该操作不可撤销。`,
      [
        { text: t`取消`, style: "cancel" },
        {
          text: t`清空`,
          style: "destructive",
          onPress: async () => {
            await clearAllHistory();
            toast.success(t`已清空传输历史`);
          },
        },
      ],
    );
  }, [clearAllHistory, t]);

  const goDetail = useCallback(
    (sessionId: string) => {
      router.push({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
      } as never);
    },
    [router],
  );

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`传输历史`} />
      <ScrollView contentContainerClassName="flex-grow gap-5 px-5 pt-2 pb-8">
        {!hasContent ? (
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
          <>
            {activeSnapshots.length > 0 ? (
              <View className="gap-2">
                <Text className="text-sm font-semibold text-foreground">
                  <Trans>活跃传输</Trans>
                </Text>
                {activeSnapshots.map((snap) => (
                  <RecentTransferRow
                    key={snap.sessionId}
                    snapshot={snap}
                    onPress={goDetail}
                  />
                ))}
              </View>
            ) : null}

            {dbHistory.length > 0 ? (
              <View className="gap-2.5">
                <View className="flex-row items-center justify-between gap-2">
                  <Text className="text-sm font-semibold text-foreground">
                    <Trans>传输历史</Trans>
                  </Text>
                  <Pressable
                    onPress={handleClear}
                    accessibilityRole="button"
                    accessibilityLabel={t`清空历史`}
                    hitSlop={6}
                    className="flex-row items-center gap-1 rounded-full px-2 py-1 active:opacity-70"
                  >
                    <Trash2 color={colors.mutedForeground} size={13} />
                    <Text className="text-[11px] text-muted-foreground">
                      <Trans>清空</Trans>
                    </Text>
                  </Pressable>
                </View>
                <FilterBar current={statusFilter} onChange={setStatusFilter} />
                {filteredHistory.length > 0 ? (
                  <View className="gap-2">
                    {filteredHistory.map((item) => (
                      <HistoryTransferRow
                        key={item.sessionId}
                        item={item}
                        onPress={goDetail}
                      />
                    ))}
                  </View>
                ) : null}
              </View>
            ) : null}
          </>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

interface FilterBarProps {
  current: StatusFilter;
  onChange: (filter: StatusFilter) => void;
}

function FilterBar({ current, onChange }: FilterBarProps) {
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerClassName="gap-2 py-1"
    >
      {FILTER_OPTIONS.map((f) => {
        const active = current === f;
        return (
          <Pressable
            key={String(f)}
            onPress={() => onChange(f)}
            accessibilityRole="button"
            className={`rounded-full px-3 py-1.5 ${
              active ? "bg-primary" : "border border-border bg-card"
            } active:opacity-70`}
          >
            <Text
              className={`text-[12px] font-medium ${
                active ? "text-primary-foreground" : "text-foreground"
              }`}
            >
              {f === "all" ? <Trans>全部</Trans> : <StatusLabel status={f} />}
            </Text>
          </Pressable>
        );
      })}
    </ScrollView>
  );
}
