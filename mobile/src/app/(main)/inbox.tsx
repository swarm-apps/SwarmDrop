import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import {
  AlertTriangle,
  ArchiveRestore,
  FileArchive,
  HardDrive,
  Inbox,
  RefreshCw,
  Search,
} from "lucide-react-native";
import { type ReactNode, useCallback, useMemo, useState } from "react";
import { ActivityIndicator, FlatList, Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
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
  EmptyState,
  HeaderIconButton,
  LIST_CONTENT_PADDING,
  Surface,
} from "@/components/mobile/screen";
import { formatBytes, formatRelativeTime } from "@/components/transfer/shared";
import { Skeleton } from "@/components/ui/skeleton";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { type InboxPreviewItem, useInboxStore } from "@/stores/inbox-store";

export default function InboxScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const [filter, setFilter] = useState<InboxFilter>("all");
  const { loading, items, action, lastError, refresh, repairMissingItems } =
    useInboxStore(
      useShallow((s) => ({
        loading: s.loading,
        items: s.items,
        action: s.action,
        lastError: s.lastError,
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
    // 带上当前筛选,避免进入关键词搜索后之前选好的类型筛选被静默丢弃。
    router.push({
      pathname: "/inbox/search",
      params: { filter },
    } as never);
  }, [router, filter]);

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

  const renderInboxItem = useCallback(
    ({ item, index }: { item: InboxPreviewItem; index: number }) => (
      <InboxRow item={item} index={index} onPress={openDetail} />
    ),
    [openDetail],
  );

  // 头部(标题栏 + 工具栏 + 筛选栏)作为 FlatList 的 ListHeaderComponent 一起滚动,
  // 列表主体走虚拟化,避免收件箱增长后整屏 .map 全量挂载。
  const listHeader = (
    <View className="gap-5 pb-5">
      <AppHeader
        title={<Trans>收件箱</Trans>}
        subtitle={<Trans>收到的内容都替你放在这儿</Trans>}
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

      {lastError ? (
        <InboxErrorBanner message={lastError} onRetry={refresh} />
      ) : null}

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
    </View>
  );

  const emptyComponent =
    loading && items.length === 0 ? (
      // 首次加载尚无数据:渲染骨架列表,避免闪一下"收件箱还是空的"假空态。
      <InboxListSkeleton />
    ) : items.length === 0 ? (
      <EmptyState
        icon={FileArchive}
        title={<Trans>收件箱还是空的</Trans>}
        description={
          <Trans>
            收到的东西会出现在这里；传输过程和续传入口在传输记录里。
          </Trans>
        }
        actionLabel={<Trans>刷新</Trans>}
        onAction={refresh}
        testID="inbox-empty-state"
      />
    ) : (
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
    );

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top"]}
      testID="inbox-screen"
    >
      <FlatList
        data={visibleItems}
        keyExtractor={inboxKeyExtractor}
        renderItem={renderInboxItem}
        showsVerticalScrollIndicator={false}
        contentContainerStyle={LIST_CONTENT_PADDING}
        ListHeaderComponent={listHeader}
        ItemSeparatorComponent={InboxItemGap}
        ListEmptyComponent={emptyComponent}
        testID="inbox-list"
      />
    </SafeAreaView>
  );
}

const inboxKeyExtractor = (item: InboxPreviewItem) => item.id;

function InboxItemGap() {
  return <View className="h-2.5" />;
}

/** 首次加载(尚无缓存数据)时的收件箱骨架列表,镜像 InboxRow 布局:图标 chip + 标题行 + 元信息行。 */
function InboxListSkeleton() {
  const { t } = useLingui();
  return (
    <View
      className="gap-2.5"
      accessible
      accessibilityLabel={t`加载中`}
      testID="inbox-loading-skeleton"
    >
      {Array.from({ length: 4 }, (_, index) => (
        <View
          // biome-ignore lint/suspicious/noArrayIndexKey: 静态占位行,无重排
          key={index}
          className="min-h-20 flex-row items-center gap-3 rounded-lg border border-border bg-card p-3.5"
        >
          <Skeleton className="size-12 rounded-xl" />
          <View className="min-w-0 flex-1 gap-1">
            <Skeleton className="h-3.5 w-1/2" />
            <Skeleton className="h-3 w-2/3" />
            <Skeleton className="h-3 w-1/3" />
          </View>
        </View>
      ))}
    </View>
  );
}

/** 把 store 的 lastError 实际渲染出来,而不是只 console.warn(镜像 Requirement: Store Errors Surfaced to the User)。 */
function InboxErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  const colors = useThemeColors();
  return (
    <View
      className="flex-row items-center gap-2.5 rounded-xl border border-destructive/30 bg-destructive/10 p-3"
      testID="inbox-refresh-error"
    >
      <AlertTriangle color={colors.destructive} size={16} />
      <Text
        className="flex-1 text-[12px] text-destructive-ink"
        numberOfLines={2}
      >
        {message}
      </Text>
      <Pressable
        onPress={onRetry}
        accessibilityRole="button"
        testID="inbox-refresh-error-retry"
        className="h-8 items-center justify-center rounded-xl border border-border bg-card px-3 active:opacity-70"
      >
        <Text className="text-[12px] font-semibold text-foreground">
          <Trans>重试</Trans>
        </Text>
      </Pressable>
    </View>
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
            <Text className="text-[15px] font-semibold tabular-nums text-primary-ink">
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
          className="min-h-11 flex-row items-center justify-between rounded-xl bg-destructive/10 px-3 active:opacity-70 disabled:opacity-50"
        >
          <View className="min-w-0 flex-1">
            <Text className="text-[12px] font-semibold text-destructive-ink">
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
