import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useLocalSearchParams, useRouter } from "expo-router";
import {
  AlertTriangle,
  ChevronLeft,
  ChevronRight,
  FileArchive,
  Search,
  X,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, TextInput, View } from "react-native";
import { useShallow } from "zustand/react/shallow";
import {
  FilterRail,
  filterInboxItems,
  getInboxFilterCounts,
  HighlightedText,
  type InboxFilter,
  InboxRow,
  InboxStatusBadges,
  matchesInboxFilter,
} from "@/components/inbox/inbox-list";
import { AppScreen, EmptyState } from "@/components/mobile/screen";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import {
  type InboxPreviewItem,
  type InboxSearchHit,
  supportsServerInboxSearch,
  useInboxStore,
} from "@/stores/inbox-store";

const VALID_FILTERS: readonly InboxFilter[] = [
  "all",
  "files",
  "images",
  "videos",
  "text",
  "attention",
  "archived",
];

/** 校验从上一屏(收件箱列表)带入的筛选参数,非法值一律回退到"全部"。 */
function parseInboxFilter(value: string | undefined): InboxFilter {
  return (VALID_FILTERS as readonly string[]).includes(value ?? "")
    ? (value as InboxFilter)
    : "all";
}

export default function InboxSearchScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const { filter: initialFilter } = useLocalSearchParams<{
    filter?: string;
  }>();
  const [query, setQuery] = useState("");
  // 从收件箱列表带入之前选好的筛选,而不是每次进入搜索都静默重置为"全部"。
  const [filter, setFilter] = useState<InboxFilter>(() =>
    parseInboxFilter(initialFilter),
  );
  // 服务端 FTS 是否可用(取决于 mobile-core 绑定);不可用则退回客户端过滤。
  const serverSearch = useMemo(() => supportsServerInboxSearch(), []);
  const {
    items,
    refresh,
    runSearch,
    clearSearch,
    searchResults,
    searching,
    lastError,
  } = useInboxStore(
    useShallow((s) => ({
      items: s.items,
      refresh: s.refresh,
      runSearch: s.runSearch,
      clearSearch: s.clearSearch,
      searchResults: s.searchResults,
      searching: s.searching,
      lastError: s.lastError,
    })),
  );

  useFocusEffect(
    useCallback(() => {
      void refresh();
    }, [refresh]),
  );

  const hasQuery = query.trim().length > 0;

  // 服务端检索:防抖触发;查询清空时退出检索态。客户端模式由 filterInboxItems 即时处理。
  useEffect(() => {
    if (!serverSearch) return;
    if (!hasQuery) {
      clearSearch();
      return;
    }
    const id = setTimeout(() => void runSearch(query, true), 250);
    return () => clearTimeout(id);
  }, [serverSearch, hasQuery, query, runSearch, clearSearch]);

  // 离开页面时清掉检索结果,避免下次进入闪现旧命中。
  useEffect(() => () => clearSearch(), [clearSearch]);

  const openDetail = useCallback(
    (itemId: string) => {
      router.push({
        pathname: "/inbox/[itemId]",
        params: { itemId },
      } as never);
    },
    [router],
  );

  const filterCounts = useMemo(() => getInboxFilterCounts(items), [items]);
  // 客户端可见项:无查询时按 filter 展示最近接收;客户端模式下叠加 query 过滤。
  const clientItems = useMemo(
    () =>
      filterInboxItems(items, {
        filter,
        query: serverSearch ? "" : query,
      }),
    [filter, items, query, serverSearch],
  );

  // FTS 命中不带 contentKind/missing/archivedAt 字段,借助已加载的 items 按 id 反查,
  // 既用来给命中行补状态徽标,也用来在客户端把命中结果收敛到当前筛选范围。
  const itemById = useMemo(
    () => new Map(items.map((item) => [item.id, item] as const)),
    [items],
  );
  const scopedSearchResults = useMemo(() => {
    if (!searchResults) return [];
    if (filter === "all") return searchResults;
    return searchResults.filter((hit) => {
      const summary = itemById.get(hit.id);
      return summary ? matchesInboxFilter(summary, filter) : true;
    });
  }, [searchResults, filter, itemById]);

  // 筛选一直可见(含 FTS 检索态),让用户能看到并调整当前生效的筛选范围。
  const showFilterRail = items.length > 0;

  return (
    <AppScreen
      scroll
      testID="inbox-search-screen"
      contentClassName="gap-4 pt-1"
    >
      <View className="min-h-14 flex-row items-center gap-2">
        <Pressable
          onPress={() => router.back()}
          accessibilityRole="button"
          accessibilityLabel={t`返回`}
          testID="inbox-search-back-button"
          className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70"
        >
          <ChevronLeft color={colors.foreground} size={21} />
        </Pressable>
        <View className="min-h-11 min-w-0 flex-1 flex-row items-center gap-2 rounded-xl bg-muted px-3">
          <Search color={colors.mutedForeground} size={16} />
          <TextInput
            autoFocus
            value={query}
            onChangeText={setQuery}
            placeholder={
              serverSearch ? t`搜索标题、来源、文件名…` : t`搜索标题或来源`
            }
            placeholderTextColor={colors.mutedForeground}
            returnKeyType="search"
            className="h-11 min-w-0 flex-1 text-[14px] text-foreground"
            testID="inbox-search-input"
          />
          {searching ? (
            <ActivityIndicator size="small" color={colors.mutedForeground} />
          ) : null}
          {query.length > 0 ? (
            <Pressable
              onPress={() => setQuery("")}
              accessibilityRole="button"
              accessibilityLabel={t`清除搜索`}
              hitSlop={8}
              className="size-7 items-center justify-center rounded-full bg-card active:opacity-70"
            >
              <X color={colors.mutedForeground} size={14} />
            </Pressable>
          ) : null}
        </View>
      </View>

      {showFilterRail ? (
        <FilterRail value={filter} counts={filterCounts} onChange={setFilter} />
      ) : null}

      <SearchBody
        items={items}
        hasQuery={hasQuery}
        query={query}
        serverSearch={serverSearch}
        searching={searching}
        searchResults={searchResults}
        scopedSearchResults={scopedSearchResults}
        itemById={itemById}
        lastError={lastError}
        clientItems={clientItems}
        onOpenDetail={openDetail}
        onRetrySearch={() => void runSearch(query, true)}
        onClear={() => {
          setQuery("");
          setFilter("all");
        }}
      />
    </AppScreen>
  );
}

function SearchBody({
  items,
  hasQuery,
  query,
  serverSearch,
  searching,
  searchResults,
  scopedSearchResults,
  itemById,
  lastError,
  clientItems,
  onOpenDetail,
  onRetrySearch,
  onClear,
}: {
  items: unknown[];
  hasQuery: boolean;
  query: string;
  serverSearch: boolean;
  searching: boolean;
  searchResults: InboxSearchHit[] | null;
  /** searchResults 按当前内容筛选收敛后的结果(用于计数与渲染)。 */
  scopedSearchResults: InboxSearchHit[];
  /** 命中 id -> 收件箱摘要,供命中行补状态徽标。 */
  itemById: Map<string, InboxPreviewItem>;
  lastError: string | null;
  clientItems: React.ComponentProps<typeof InboxRow>["item"][];
  onOpenDetail: (itemId: string) => void;
  onRetrySearch: () => void;
  onClear: () => void;
}) {
  const useFts = serverSearch && hasQuery;
  const resultCount = useFts ? scopedSearchResults.length : clientItems.length;

  return (
    <>
      <View className="flex-row items-center justify-between px-1">
        <Text className="text-[13px] font-semibold text-foreground">
          {hasQuery ? <Trans>搜索结果</Trans> : <Trans>最近接收</Trans>}
        </Text>
        {items.length > 0 ? (
          <Text className="text-[11px] text-muted-foreground">
            {resultCount} <Trans>条</Trans>
          </Text>
        ) : null}
      </View>

      {items.length === 0 ? (
        <EmptyState
          icon={FileArchive}
          title={<Trans>收件箱还是空的</Trans>}
          description={<Trans>完成接收后，可以在这里搜索标题或来源。</Trans>}
          testID="inbox-search-empty-state"
        />
      ) : useFts ? (
        searching && (searchResults === null || searchResults.length === 0) ? (
          <View
            className="min-h-48 items-center justify-center"
            testID="inbox-search-loading"
          >
            <ActivityIndicator size="small" />
          </View>
        ) : lastError && searchResults === null ? (
          <EmptyState
            icon={AlertTriangle}
            title={<Trans>搜索失败</Trans>}
            description={lastError}
            actionLabel={<Trans>重试</Trans>}
            onAction={onRetrySearch}
            className="min-h-48"
            testID="inbox-search-error-state"
          />
        ) : scopedSearchResults.length > 0 ? (
          <View className="gap-2.5" testID="inbox-search-results">
            {scopedSearchResults.map((hit, index) => (
              <InboxHitRow
                key={hit.id}
                hit={hit}
                index={index}
                query={query}
                item={itemById.get(hit.id)}
                onPress={onOpenDetail}
              />
            ))}
          </View>
        ) : (
          <EmptyState
            icon={Search}
            title={<Trans>没有匹配的内容</Trans>}
            description={<Trans>换一个关键词试试。</Trans>}
            className="min-h-48"
            testID="inbox-search-no-results"
          />
        )
      ) : clientItems.length === 0 ? (
        <EmptyState
          icon={Search}
          title={<Trans>没有匹配的内容</Trans>}
          description={<Trans>换一个关键词，或切换上方筛选范围。</Trans>}
          actionLabel={<Trans>清除筛选</Trans>}
          onAction={onClear}
          className="min-h-48"
          testID="inbox-search-no-results"
        />
      ) : (
        <View className="gap-2.5" testID="inbox-search-results">
          {clientItems.map((item, index) => (
            <InboxRow
              key={item.id}
              item={item}
              index={index}
              onPress={onOpenDetail}
              highlight={query}
            />
          ))}
        </View>
      )}
    </>
  );
}

/**
 * FTS 命中行:展示标题 / 来源 / 命中片段(均按 query 高亮)。
 * 状态徽标(缺失/已归档/AI 代理)通过 `item`(按 id 从已加载的收件箱列表反查)与
 * 浏览态 InboxRow 保持一致——FTS 命中本身不带这些字段。
 */
function InboxHitRow({
  hit,
  index,
  query,
  item,
  onPress,
}: {
  hit: InboxSearchHit;
  index: number;
  query: string;
  item?: InboxPreviewItem;
  onPress: (itemId: string) => void;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      accessibilityRole="button"
      onPress={() => onPress(hit.id)}
      testID={`inbox-search-hit-${index}`}
      className="min-h-20 flex-row items-center gap-3 rounded-lg border border-border bg-card p-3.5 active:bg-muted/50"
    >
      <View className="size-12 items-center justify-center rounded-xl bg-primary/10">
        <FileArchive color={colors.primary} size={20} />
      </View>
      <View className="min-w-0 flex-1 gap-1">
        <View className="flex-row items-center gap-2">
          <HighlightedText
            text={hit.title}
            query={query}
            className="min-w-0 flex-1 text-[14px] font-semibold text-foreground"
            numberOfLines={1}
          />
          {item ? <InboxStatusBadges item={item} /> : null}
        </View>
        <View className="flex-row items-center gap-1.5">
          <HighlightedText
            text={hit.sourceName}
            query={query}
            className="min-w-0 flex-1 text-[11px] text-muted-foreground"
            numberOfLines={1}
          />
          <Text className="text-[11px] text-muted-foreground">·</Text>
          <Text className="text-[11px] text-muted-foreground">
            {hit.itemCount} <Trans>项</Trans>
          </Text>
        </View>
        {hit.snippet ? (
          <HighlightedText
            text={hit.snippet}
            query={query}
            className="text-[11px] text-muted-foreground"
            numberOfLines={1}
          />
        ) : null}
      </View>
      <ChevronRight color={colors.mutedForeground} size={17} />
    </Pressable>
  );
}
