import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import {
  ArchiveRestore,
  FileArchive,
  HardDrive,
  Inbox,
  RefreshCw,
  Search,
} from "lucide-react-native";
import { type ReactNode, useCallback, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { useShallow } from "zustand/react/shallow";
import {
  FilterRail,
  filterInboxItems,
  getInboxFilterCounts,
  type InboxFilter,
  InboxRow,
} from "@/components/inbox/inbox-list";
import {
  AppHeader,
  AppScreen,
  EmptyState,
  HeaderIconButton,
  Surface,
} from "@/components/mobile/screen";
import { formatBytes, formatRelativeTime } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { useInboxStore } from "@/stores/inbox-store";

export default function InboxScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const [filter, setFilter] = useState<InboxFilter>("all");
  const { loading, items, action, refresh, repairMissingItems } = useInboxStore(
    useShallow((s) => ({
      loading: s.loading,
      items: s.items,
      action: s.action,
      refresh: s.refresh,
      repairMissingItems: s.repairMissingItems,
    })),
  );

  useFocusEffect(
    useCallback(() => {
      void refresh();
    }, [refresh]),
  );

  const openDetail = useCallback(
    (itemId: string) => {
      router.push({
        pathname: "/inbox/[itemId]",
        params: { itemId },
      } as never);
    },
    [router],
  );

  const openSearch = useCallback(() => {
    router.push("/inbox/search" as never);
  }, [router]);

  const repair = useCallback(async () => {
    try {
      const count = await repairMissingItems();
      toast.success(
        count > 0 ? t`已修复 ${count} 条收件箱记录` : t`收件箱已是最新`,
      );
    } catch (err) {
      toast.error(t`修复收件箱失败`, err);
    }
  }, [repairMissingItems, t]);
  const isRepairing = action === "repair";

  const inboxStats = useMemo(() => {
    const totalBytes = items.reduce((sum, item) => sum + item.totalSize, 0n);
    const latest = items.reduce<bigint | null>((current, item) => {
      const receivedAt = BigInt(item.receivedAt);
      return current === null || receivedAt > current ? receivedAt : current;
    }, null);
    return { totalBytes, latest };
  }, [items]);

  const filterCounts = useMemo(() => getInboxFilterCounts(items), [items]);

  const visibleItems = useMemo(
    () => filterInboxItems(items, { filter }),
    [filter, items],
  );

  return (
    <AppScreen scroll testID="inbox-screen" contentClassName="gap-5 pt-1">
      <AppHeader
        title={<Trans>收件箱</Trans>}
        subtitle={<Trans>已接收内容会在这里归档</Trans>}
        testID="inbox-header"
        right={
          <View className="flex-row gap-2">
            <HeaderIconButton
              icon={Search}
              label={t`搜索收件箱`}
              onPress={openSearch}
              testID="inbox-open-search-button"
            />
            <HeaderIconButton
              icon={RefreshCw}
              label={t`刷新`}
              onPress={refresh}
              testID="inbox-refresh-button"
            />
          </View>
        }
      />

      <InboxToolbar
        count={items.length}
        totalBytes={inboxStats.totalBytes}
        latest={inboxStats.latest}
        loading={loading || isRepairing}
        attentionCount={filterCounts.attention}
        repairing={isRepairing}
        onRepair={repair}
      />

      {items.length > 0 ? (
        <FilterRail value={filter} counts={filterCounts} onChange={setFilter} />
      ) : null}

      {items.length === 0 ? (
        <EmptyState
          icon={FileArchive}
          title={<Trans>收件箱还是空的</Trans>}
          description={
            <Trans>
              完成的接收内容会在这里出现；传输过程和恢复入口在活动页查看。
            </Trans>
          }
          actionLabel={loading ? <Trans>刷新中</Trans> : <Trans>刷新</Trans>}
          onAction={refresh}
          actionLoading={loading}
          actionDisabled={loading}
          testID="inbox-empty-state"
        />
      ) : visibleItems.length === 0 ? (
        <EmptyState
          icon={FileArchive}
          title={<Trans>当前筛选下没有内容</Trans>}
          description={
            <Trans>切换上方筛选范围，或进入搜索页查找历史内容。</Trans>
          }
          actionLabel={<Trans>搜索</Trans>}
          onAction={openSearch}
          className="min-h-48"
          testID="inbox-filter-empty-state"
        />
      ) : (
        <View className="gap-2.5" testID="inbox-list">
          {visibleItems.map((item, index) => (
            <InboxRow
              key={item.id}
              item={item}
              index={index}
              onPress={openDetail}
            />
          ))}
        </View>
      )}
    </AppScreen>
  );
}

function InboxToolbar({
  count,
  totalBytes,
  latest,
  loading,
  attentionCount,
  repairing,
  onRepair,
}: {
  count: number;
  totalBytes: bigint;
  latest: bigint | null;
  loading: boolean;
  attentionCount: number;
  repairing: boolean;
  onRepair: () => void;
}) {
  const colors = useThemeColors();

  return (
    <Surface className="gap-4" testID="inbox-toolbar">
      <View className="flex-row items-start justify-between gap-4">
        <View className="min-w-0 flex-1">
          <View className="flex-row items-center gap-2">
            <View className="size-8 items-center justify-center rounded-full bg-primary/10">
              <Inbox color={colors.primary} size={16} />
            </View>
            <Text className="text-[15px] font-semibold text-foreground">
              <Trans>接收内容库</Trans>
            </Text>
          </View>
          <Text className="mt-1.5 text-[12px] leading-5 text-muted-foreground">
            <Trans>浏览最近接收的文件、文本和合集。</Trans>
          </Text>
        </View>
        <View className="items-end gap-1">
          <View className="flex-row items-center gap-1.5">
            {loading ? <ActivityIndicator color={colors.primary} /> : null}
            <Text className="text-[26px] font-bold tabular-nums text-foreground">
              {count}
            </Text>
          </View>
          <Text className="text-[11px] text-muted-foreground">
            <Trans>条内容</Trans>
          </Text>
        </View>
      </View>

      <View className="flex-row gap-2">
        <StatPill
          icon={HardDrive}
          label={<Trans>容量</Trans>}
          value={formatBytes(totalBytes)}
        />
        <StatPill
          icon={RefreshCw}
          label={<Trans>最近</Trans>}
          value={
            latest === null ? <Trans>暂无</Trans> : formatRelativeTime(latest)
          }
        />
      </View>

      {attentionCount > 0 ? (
        <Pressable
          onPress={onRepair}
          disabled={repairing}
          accessibilityRole="button"
          testID="inbox-repair-button"
          className="min-h-11 flex-row items-center justify-between rounded-xl bg-destructive/10 px-3 active:opacity-70 disabled:opacity-55"
        >
          <View className="min-w-0 flex-1">
            <Text className="text-[12px] font-semibold text-destructive">
              <Trans>发现 {attentionCount} 条异常内容</Trans>
            </Text>
            <Text className="mt-0.5 text-[11px] text-muted-foreground">
              <Trans>检查本地文件并更新收件箱状态</Trans>
            </Text>
          </View>
          {repairing ? (
            <ActivityIndicator color={colors.destructive} />
          ) : (
            <ArchiveRestore color={colors.destructive} size={17} />
          )}
        </Pressable>
      ) : null}
    </Surface>
  );
}

function StatPill({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof HardDrive;
  label: ReactNode;
  value: ReactNode;
}) {
  const colors = useThemeColors();
  return (
    <View className="min-h-12 flex-1 flex-row items-center gap-2 rounded-xl bg-primary/5 px-3">
      <Icon color={colors.primary} size={15} />
      <View className="min-w-0 flex-1">
        <Text className="text-[10px] text-muted-foreground">{label}</Text>
        <Text
          className="text-[12px] font-semibold text-foreground"
          numberOfLines={1}
        >
          {value}
        </Text>
      </View>
    </View>
  );
}
