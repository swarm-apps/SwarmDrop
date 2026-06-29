import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import { ChevronLeft, FileArchive, Search, X } from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { Pressable, TextInput, View } from "react-native";
import { useShallow } from "zustand/react/shallow";
import {
  FilterRail,
  filterInboxItems,
  getInboxFilterCounts,
  type InboxFilter,
  InboxRow,
} from "@/components/inbox/inbox-list";
import { AppScreen, EmptyState } from "@/components/mobile/screen";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { useInboxStore } from "@/stores/inbox-store";

export default function InboxSearchScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<InboxFilter>("all");
  const { items, refresh } = useInboxStore(
    useShallow((s) => ({
      items: s.items,
      refresh: s.refresh,
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

  const filterCounts = useMemo(() => getInboxFilterCounts(items), [items]);
  const visibleItems = useMemo(
    () => filterInboxItems(items, { filter, query }),
    [filter, items, query],
  );
  const hasQuery = query.trim().length > 0;

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
            placeholder={t`搜索标题或来源`}
            placeholderTextColor={colors.mutedForeground}
            returnKeyType="search"
            className="h-11 min-w-0 flex-1 text-[14px] text-foreground"
            testID="inbox-search-input"
          />
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

      {items.length > 0 ? (
        <FilterRail value={filter} counts={filterCounts} onChange={setFilter} />
      ) : null}

      <View className="flex-row items-center justify-between px-1">
        <Text className="text-[13px] font-semibold text-foreground">
          {hasQuery ? <Trans>搜索结果</Trans> : <Trans>最近接收</Trans>}
        </Text>
        {items.length > 0 ? (
          <Text className="text-[11px] text-muted-foreground">
            {visibleItems.length} <Trans>条</Trans>
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
      ) : visibleItems.length === 0 ? (
        <EmptyState
          icon={Search}
          title={<Trans>没有匹配的内容</Trans>}
          description={<Trans>换一个关键词，或切换上方筛选范围。</Trans>}
          actionLabel={<Trans>清除筛选</Trans>}
          onAction={() => {
            setQuery("");
            setFilter("all");
          }}
          className="min-h-48"
          testID="inbox-search-no-results"
        />
      ) : (
        <View className="gap-2.5" testID="inbox-search-results">
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
