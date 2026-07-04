import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import { Activity, Search, SearchX, Trash2 } from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { SectionList, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { MobileTransferProjection } from "react-native-swarmdrop-core";
import { ActivityProjectionCard } from "@/components/activity-projection-card";
import { FilterChip, FilterChipRail } from "@/components/filter-chip";
import {
  EmptyState,
  HeaderIconButton,
  InlineEmptyState,
  LIST_CONTENT_PADDING,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import {
  groupTransferProjections,
  projectionDirection,
  projectionGroup,
} from "@/core/transfer-types";
import { toast } from "@/lib/toast";
import { useInboxStore } from "@/stores/inbox-store";
import { useTransferStore } from "@/stores/transfer-store";

const ACTIVITY_FILTERS = ["all", "receive", "send", "attention"] as const;

type ActivityFilter = (typeof ACTIVITY_FILTERS)[number];

const ACTIVITY_FILTER_LABELS: Record<ActivityFilter, React.ReactNode> = {
  all: <Trans>全部</Trans>,
  receive: <Trans>收到</Trans>,
  send: <Trans>发出</Trans>,
  attention: <Trans>需要注意</Trans>,
};

/** 筛选是纯内存过滤:projections 本就全量在手,不值得为此过 FFI。搜索在 /transfer/search。 */
function matchesActivityFilter(
  projection: MobileTransferProjection,
  filter: ActivityFilter,
): boolean {
  if (filter === "all") return true;
  if (filter === "attention")
    return projectionGroup(projection) === "attention";
  return projectionDirection(projection) === filter;
}

export default function ActivityScreen() {
  const router = useRouter();
  const { t } = useLingui();
  const projections = useTransferStore((s) => s.projections);
  const progressBySession = useTransferStore((s) => s.progressBySession);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  const clearAllHistory = useTransferStore((s) => s.clearAllHistory);
  const resumeHistoryItem = useTransferStore((s) => s.resumeHistoryItem);
  const inboxItems = useInboxStore((s) => s.items);
  const [clearOpen, setClearOpen] = useState(false);
  const [filter, setFilter] = useState<ActivityFilter>("all");

  useFocusEffect(
    useCallback(() => {
      void loadProjections();
    }, [loadProjections]),
  );

  const allProjections = useMemo(
    () => Object.values(projections),
    [projections],
  );

  // 计数按全量算(不随筛选变化),让用户在任何筛选下都能看到全貌;谓词与列表过滤共用一份。
  const filterCounts = useMemo(
    () =>
      Object.fromEntries(
        ACTIVITY_FILTERS.map((f) => [
          f,
          allProjections.filter((p) => matchesActivityFilter(p, f)).length,
        ]),
      ) as Record<ActivityFilter, number>,
    [allProjections],
  );

  const grouped = useMemo(
    () =>
      groupTransferProjections(
        allProjections.filter((p) => matchesActivityFilter(p, filter)),
      ),
    [allProjections, filter],
  );

  // 会话 → 收件箱记录 反查:只有"接收且已落库"的会话能命中,用于已完成接收卡片的
  // 「在收件箱查看」深链。冷启动 inbox 尚未加载时 map 为空,深链自然缺席(防御式)。
  const inboxItemIdBySession = useMemo(() => {
    const map = new Map<string, string>();
    for (const inboxItem of inboxItems) {
      if (inboxItem.transferSessionId) {
        map.set(inboxItem.transferSessionId, inboxItem.id);
      }
    }
    return map;
  }, [inboxItems]);

  // 4 个分组 → SectionList sections;数据只依赖 grouped(不含每 tick 变化的 progress),
  // 进度经 extraData 注入、按会话 memo 的卡片只重渲染真正变化的那一条。
  // 顺序按"可行动优先":正在进行 → 可恢复(有恢复按钮) → 需要注意(只需知晓) → 已完成。
  // showStatusBadge:分组标题与卡片状态恒同名的组(正在进行/已完成)关掉徽章,不复读;
  // 混合状态的组(需要注意=失败/取消/拒绝,可恢复=暂停/中断)保留徽章区分具体状态。
  const sections = useMemo(() => {
    const defs = [
      {
        key: "active",
        title: <Trans>正在进行</Trans>,
        data: grouped.active,
        showProgress: true,
        resume: false,
        showStatusBadge: false,
        testID: "activity-section-active",
      },
      {
        key: "recoverable",
        title: <Trans>可恢复</Trans>,
        data: grouped.recoverable,
        showProgress: true,
        resume: true,
        showStatusBadge: true,
        testID: "activity-section-recoverable",
      },
      {
        key: "attention",
        title: <Trans>需要注意</Trans>,
        data: grouped.attention,
        showProgress: false,
        resume: false,
        showStatusBadge: true,
        testID: "activity-section-attention",
      },
      {
        key: "completed",
        title: <Trans>已完成</Trans>,
        data: grouped.completed,
        showProgress: false,
        resume: false,
        showStatusBadge: false,
        testID: "activity-section-completed",
      },
    ];
    return defs.filter((s) => s.data.length > 0);
  }, [grouped]);

  const goDetail = useCallback(
    (sessionId: string) => {
      router.push({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
      } as never);
    },
    [router],
  );

  const openSearch = useCallback(() => {
    router.push("/transfer/search" as never);
  }, [router]);

  const openInboxItem = useCallback(
    (itemId: string) => {
      router.push({
        pathname: "/inbox/[itemId]",
        params: { itemId },
      } as never);
    },
    [router],
  );

  const resume = useCallback(
    async (sessionId: string) => {
      try {
        const nextSessionId = await resumeHistoryItem(sessionId);
        toast.success(t`已开始恢复传输`);
        goDetail(nextSessionId);
      } catch (err) {
        toast.error(
          t`恢复失败`,
          err instanceof Error ? err.message : String(err),
        );
      }
    },
    [goDetail, resumeHistoryItem, t],
  );

  const performClear = useCallback(async () => {
    await clearAllHistory();
    toast.success(t`已清空传输记录`);
  }, [clearAllHistory, t]);

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top"]}
      testID="activity-screen"
    >
      <SectionList
        sections={sections}
        keyExtractor={activityKeyExtractor}
        extraData={progressBySession}
        stickySectionHeadersEnabled={false}
        showsVerticalScrollIndicator={false}
        contentContainerStyle={LIST_CONTENT_PADDING}
        ListHeaderComponent={
          <View className="gap-4">
            <SettingsHeader
              title={t`传输记录`}
              right={
                <View className="flex-row gap-2">
                  <HeaderIconButton
                    icon={Search}
                    label={t`搜索传输记录`}
                    onPress={openSearch}
                    testID="activity-open-search-button"
                  />
                  <HeaderIconButton
                    icon={Trash2}
                    label={t`清空传输记录`}
                    onPress={() => setClearOpen(true)}
                    testID="activity-clear-button"
                  />
                </View>
              }
            />
            <Text className="px-1 text-[12px] text-muted-foreground">
              <Trans>每一笔传输的过程都记在这里；收好的东西请到收件箱找</Trans>
            </Text>
            <FilterChipRail testID="activity-filter-rail">
              {ACTIVITY_FILTERS.map((f) => (
                <FilterChip
                  key={f}
                  active={filter === f}
                  label={ACTIVITY_FILTER_LABELS[f]}
                  count={filterCounts[f]}
                  onPress={() => setFilter(f)}
                  testID={`activity-filter-${f}`}
                />
              ))}
            </FilterChipRail>
          </View>
        }
        renderSectionHeader={({ section }) => (
          <View className="pb-2.5 pt-5" testID={section.testID}>
            <Text className="text-[15px] font-semibold text-foreground">
              {section.title}
            </Text>
            {section.key === "completed" ? (
              <Text className="mt-1 text-[11px] text-muted-foreground">
                <Trans>收到的内容已放进收件箱</Trans>
              </Text>
            ) : null}
          </View>
        )}
        renderItem={({ item, section }) => (
          <ActivityProjectionCard
            projection={item}
            progress={progressBySession[item.sessionId]}
            showProgress={section.showProgress}
            showStatusBadge={section.showStatusBadge}
            onPress={goDetail}
            onResume={section.resume ? resume : undefined}
            inboxItemId={
              section.key === "completed"
                ? inboxItemIdBySession.get(item.sessionId)
                : undefined
            }
            onOpenInbox={openInboxItem}
          />
        )}
        ItemSeparatorComponent={ActivityItemGap}
        ListEmptyComponent={
          <View className="pt-5">
            {allProjections.length === 0 ? (
              <EmptyState
                icon={Activity}
                title={<Trans>暂无传输记录</Trans>}
                description={
                  <Trans>从设备页发送文件，或接收其他设备发来的内容。</Trans>
                }
                testID="activity-empty-state"
              />
            ) : (
              // 有记录但被筛选排空 —— 与真空态区分,指回筛选条件本身。
              <InlineEmptyState
                icon={SearchX}
                title={<Trans>没有匹配的传输记录</Trans>}
                description={<Trans>换个筛选条件试试</Trans>}
                testID="activity-filter-empty-state"
              />
            )}
          </View>
        }
      />

      <ConfirmDialog
        open={clearOpen}
        onOpenChange={setClearOpen}
        title={<Trans>清空传输记录</Trans>}
        description={
          <Trans>
            只清空传输过程记录；收件箱里已收到的内容不受影响。该操作不可撤销。
          </Trans>
        }
        actionLabel={<Trans>清空</Trans>}
        destructive
        onAction={performClear}
        contentTestID="activity-clear-confirmation"
        cancelTestID="activity-clear-cancel-button"
        actionTestID="activity-clear-confirm-button"
      />
    </SafeAreaView>
  );
}

const activityKeyExtractor = (item: MobileTransferProjection) => item.sessionId;

function ActivityItemGap() {
  return <View className="h-2" />;
}
