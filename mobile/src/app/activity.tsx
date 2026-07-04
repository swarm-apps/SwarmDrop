import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import { Activity, Trash2 } from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { Pressable, SectionList, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { MobileTransferProjection } from "react-native-swarmdrop-core";
import { ActivityProjectionCard } from "@/components/activity-projection-card";
import { EmptyState, LIST_CONTENT_PADDING } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { groupTransferProjections } from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { useInboxStore } from "@/stores/inbox-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function ActivityScreen() {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();
  const projections = useTransferStore((s) => s.projections);
  const progressBySession = useTransferStore((s) => s.progressBySession);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  const clearAllHistory = useTransferStore((s) => s.clearAllHistory);
  const resumeHistoryItem = useTransferStore((s) => s.resumeHistoryItem);
  const inboxItems = useInboxStore((s) => s.items);
  const [clearOpen, setClearOpen] = useState(false);

  useFocusEffect(
    useCallback(() => {
      void loadProjections();
    }, [loadProjections]),
  );

  const grouped = useMemo(
    () => groupTransferProjections(Object.values(projections)),
    [projections],
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
  const sections = useMemo(() => {
    const defs = [
      {
        key: "active",
        title: <Trans>正在进行</Trans>,
        data: grouped.active,
        showProgress: true,
        resume: false,
        testID: "activity-section-active",
      },
      {
        key: "attention",
        title: <Trans>需要注意</Trans>,
        data: grouped.attention,
        showProgress: false,
        resume: false,
        testID: "activity-section-attention",
      },
      {
        key: "recoverable",
        title: <Trans>可恢复</Trans>,
        data: grouped.recoverable,
        showProgress: true,
        resume: true,
        testID: "activity-section-recoverable",
      },
      {
        key: "completed",
        title: <Trans>已完成</Trans>,
        data: grouped.completed,
        showProgress: false,
        resume: false,
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
          <View className="gap-5">
            <SettingsHeader
              title={t`传输记录`}
              right={
                <Pressable
                  onPress={() => setClearOpen(true)}
                  accessibilityRole="button"
                  accessibilityLabel={t`清空传输记录`}
                  testID="activity-clear-button"
                  className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70"
                >
                  <Trash2 color={colors.mutedForeground} size={19} />
                </Pressable>
              }
            />
            <Text className="px-1 text-[12px] text-muted-foreground">
              <Trans>每一笔传输的过程都记在这里；收好的东西请到收件箱找</Trans>
            </Text>
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
            <EmptyState
              icon={Activity}
              title={<Trans>暂无传输记录</Trans>}
              description={
                <Trans>从设备页发送文件，或接收其他设备发来的内容。</Trans>
              }
              testID="activity-empty-state"
            />
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
