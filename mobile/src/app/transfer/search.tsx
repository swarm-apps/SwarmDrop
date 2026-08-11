import { useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import { Search, SearchX } from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { FlatList } from "react-native";
import {
  ActivityProjectionCard,
  transferSessionKey,
} from "@/components/activity-projection-card";
import {
  AppScreen,
  InlineEmptyState,
  LIST_CONTENT_PADDING_UNDER_HEADER,
  ListItemGap,
} from "@/components/mobile/screen";
import { SearchHeader } from "@/components/search-header";
import { compareByTimelineDesc } from "@swarmdrop/shared-view";

import {
  projectionMatchesQuery,
  shouldShowProgress,
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
  const progressBySession = useTransferStore((s) => s.progressBySession);
  const publishingBySession = useTransferStore((s) => s.publishingBySession);
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
      .sort(compareByTimelineDesc);
  }, [projections, trimmedQuery]);

  const listExtraData = useMemo(
    () => [progressBySession, publishingBySession],
    [progressBySession, publishingBySession],
  );

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
    <AppScreen
      testID="transfer-search-screen"
      // 搜索头进 header 槽:它带返回与输入框,跟着结果滚走的话,改个关键词还得先翻回顶部。
      header={
        <SearchHeader
          value={query}
          onChangeText={setQuery}
          placeholder={t`搜索设备或文件名`}
          inputLabel={t`搜索传输记录`}
          testIDPrefix="transfer-search"
        />
      }
      bare
    >
      <FlatList
        data={results}
        keyExtractor={transferSessionKey}
        showsVerticalScrollIndicator={false}
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={LIST_CONTENT_PADDING_UNDER_HEADER}
        // 两张高频表都要进来：发布期没有新的进度事件（字节已收完），只挂
        // progressBySession 的话「正在保存」这一态永远画不出来。
        extraData={listExtraData}
        renderItem={({ item }) => (
          <ActivityProjectionCard
            projection={item}
            progress={progressBySession[item.sessionId]}
            publishing={publishingBySession[item.sessionId]}
            showProgress={shouldShowProgress(item)}
            onPress={goDetail}
          />
        )}
        ItemSeparatorComponent={ListItemGap}
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
    </AppScreen>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
