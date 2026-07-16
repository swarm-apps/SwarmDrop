import { useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import { Search, SearchX } from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { FlatList, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { MobileTransferProjection } from "react-native-swarmdrop-core";
import { ActivityProjectionCard } from "@/components/activity-projection-card";
import {
  InlineEmptyState,
  LIST_CONTENT_PADDING,
} from "@/components/mobile/screen";
import { SearchHeader } from "@/components/search-header";
import {
  compareProjectionsByUpdatedAtDesc,
  projectionMatchesQuery,
} from "@/core/transfer-types";
import { useTransferStore } from "@/stores/transfer-store";

/**
 * 传输记录搜索页 —— 与收件箱搜索同一交互形态(放大镜入口 → 独立页 + autoFocus)。
 * 区别:projections 全量在内存,输入即过滤,无 FTS/加载态。
 */
export default function TransferSearchScreen() {
  const router = useRouter();
  const { t } = useLingui();
  const projections = useTransferStore((s) => s.projections);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  const [query, setQuery] = useState("");

  useFocusEffect(
    useCallback(() => {
      void loadProjections();
    }, [loadProjections]),
  );

  const trimmedQuery = query.trim();

  // 命中结果拍平按时间倒序:搜索场景没有分组语境,卡片保留状态徽章承担状态信息。
  const results = useMemo(() => {
    if (!trimmedQuery) return [];
    return Object.values(projections)
      .filter((p) => projectionMatchesQuery(p, trimmedQuery))
      .sort(compareProjectionsByUpdatedAtDesc);
  }, [projections, trimmedQuery]);

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
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top"]}
      testID="transfer-search-screen"
    >
      <FlatList
        data={results}
        keyExtractor={searchKeyExtractor}
        showsVerticalScrollIndicator={false}
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={LIST_CONTENT_PADDING}
        ListHeaderComponent={
          <View className="pb-4">
            <SearchHeader
              value={query}
              onChangeText={setQuery}
              placeholder={t`搜索设备或文件名`}
              inputLabel={t`搜索传输记录`}
              testIDPrefix="transfer-search"
            />
          </View>
        }
        renderItem={({ item }) => (
          <ActivityProjectionCard projection={item} onPress={goDetail} />
        )}
        ItemSeparatorComponent={SearchItemGap}
        ListEmptyComponent={
          trimmedQuery ? (
            <InlineEmptyState
              icon={SearchX}
              title={t`没有匹配的传输记录`}
              description={t`换个关键词试试`}
              testID="transfer-search-empty-state"
            />
          ) : (
            <InlineEmptyState
              icon={Search}
              title={t`搜索传输记录`}
              description={t`输入设备名或文件名，结果即时出现`}
              testID="transfer-search-hint"
            />
          )
        }
      />
    </SafeAreaView>
  );
}

const searchKeyExtractor = (item: MobileTransferProjection) => item.sessionId;

function SearchItemGap() {
  return <View className="h-2" />;
}
