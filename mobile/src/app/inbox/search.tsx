import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import {
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
} from "@/components/inbox/inbox-list";
import { AppScreen, EmptyState } from "@/components/mobile/screen";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import {
  type InboxSearchHit,
  supportsServerInboxSearch,
  useInboxStore,
} from "@/stores/inbox-store";

export default function InboxSearchScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<InboxFilter>("all");
  // 服务端 FTS 是否可用(取决于 mobile-core 绑定);不可用则退回客户端过滤。
  const serverSearch = useMemo(() => supportsServerInboxSearch(), []);
  const { items, refresh, runSearch, clearSearch, searchResults, searching } =
    useInboxStore(
      useShallow((s) => ({
        items: s.items,
        refresh: s.refresh,
        runSearch: s.runSearch,
        clearSearch: s.clearSearch,
        searchResults: s.searchResults,
        searching: s.searching,
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

  // FTS 模式且有查询时,内容筛选不适用于命中项,隐藏 FilterRail。
  const showFilterRail = items.length > 0 && (!hasQuery || !serverSearch);

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
        clientItems={clientItems}
        onOpenDetail={openDetail}
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
  clientItems,
  onOpenDetail,
  onClear,
}: {
  items: unknown[];
  hasQuery: boolean;
  query: string;
  serverSearch: boolean;
  searching: boolean;
  searchResults: InboxSearchHit[] | null;
  clientItems: React.ComponentProps<typeof InboxRow>["item"][];
  onOpenDetail: (itemId: string) => void;
  onClear: () => void;
}) {
  const useFts = serverSearch && hasQuery;
  const resultCount = useFts
    ? (searchResults?.length ?? 0)
    : clientItems.length;

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
        ) : searchResults && searchResults.length > 0 ? (
          <View className="gap-2.5" testID="inbox-search-results">
            {searchResults.map((hit, index) => (
              <InboxHitRow
                key={hit.id}
                hit={hit}
                index={index}
                query={query}
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

/** FTS 命中行:展示标题 / 来源 / 命中片段(均按 query 高亮)。 */
function InboxHitRow({
  hit,
  index,
  query,
  onPress,
}: {
  hit: InboxSearchHit;
  index: number;
  query: string;
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
        <HighlightedText
          text={hit.title}
          query={query}
          className="text-[14px] font-semibold text-foreground"
          numberOfLines={1}
        />
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
