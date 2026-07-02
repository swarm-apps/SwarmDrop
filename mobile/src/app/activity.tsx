import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import { Activity, Trash2 } from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { Pressable, View } from "react-native";
import type {
  MobileTransferProgress,
  MobileTransferProjection,
} from "react-native-swarmdrop-core";
import { ActivityProjectionCard } from "@/components/activity-projection-card";
import { AppScreen, EmptyState } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { groupTransferProjections } from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
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
  const hasContent = Object.values(grouped).some((items) => items.length > 0);

  const goDetail = useCallback(
    (sessionId: string) => {
      router.push({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
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
    toast.success(t`已清空传输活动`);
  }, [clearAllHistory, t]);

  return (
    <AppScreen scroll testID="activity-screen" contentClassName="gap-5 pt-1">
      <SettingsHeader
        title={t`活动`}
        right={
          <Pressable
            onPress={() => setClearOpen(true)}
            accessibilityRole="button"
            accessibilityLabel={t`清空活动`}
            testID="activity-clear-button"
            className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70"
          >
            <Trash2 color={colors.mutedForeground} size={19} />
          </Pressable>
        }
      />

      <Text className="px-1 text-[12px] text-muted-foreground">
        <Trans>每一笔传输的实时进度与历史记录都在这里</Trans>
      </Text>

      {hasContent ? (
        <>
          <ProjectionSection
            title={<Trans>正在进行</Trans>}
            testID="activity-section-active"
            projections={grouped.active}
            progressBySession={progressBySession}
            showProgress
            onPress={goDetail}
          />
          <ProjectionSection
            title={<Trans>需要注意</Trans>}
            testID="activity-section-attention"
            projections={grouped.attention}
            progressBySession={progressBySession}
            onPress={goDetail}
          />
          <ProjectionSection
            title={<Trans>可恢复</Trans>}
            testID="activity-section-recoverable"
            projections={grouped.recoverable}
            progressBySession={progressBySession}
            showProgress
            onPress={goDetail}
            onResume={resume}
          />
          <ProjectionSection
            title={<Trans>已完成</Trans>}
            testID="activity-section-completed"
            projections={grouped.completed}
            progressBySession={progressBySession}
            onPress={goDetail}
          />
        </>
      ) : (
        <EmptyState
          icon={Activity}
          title={<Trans>暂无传输活动</Trans>}
          description={
            <Trans>从设备页发送文件，或接收其他设备发来的内容。</Trans>
          }
          testID="activity-empty-state"
        />
      )}

      <ConfirmDialog
        open={clearOpen}
        onOpenChange={setClearOpen}
        title={<Trans>清空传输活动</Trans>}
        description={
          <Trans>
            这会删除传输过程记录，不会作为收件箱内容删除入口。该操作不可撤销。
          </Trans>
        }
        actionLabel={<Trans>清空</Trans>}
        destructive
        onAction={performClear}
        contentTestID="activity-clear-confirmation"
        cancelTestID="activity-clear-cancel-button"
        actionTestID="activity-clear-confirm-button"
      />
    </AppScreen>
  );
}

function ProjectionSection({
  title,
  testID,
  projections,
  progressBySession,
  showProgress,
  onPress,
  onResume,
}: {
  title: React.ReactNode;
  testID: string;
  projections: MobileTransferProjection[];
  progressBySession: Record<string, MobileTransferProgress>;
  showProgress?: boolean;
  onPress: (sessionId: string) => void;
  onResume?: (sessionId: string) => void;
}) {
  // 空分组直接不渲染 —— 不再为"展示 1 条记录"先铺几张空占位卡。
  if (projections.length === 0) return null;
  return (
    <View className="gap-2.5" testID={testID}>
      <Text className="text-[15px] font-semibold text-foreground">{title}</Text>
      <View className="gap-2">
        {projections.map((projection) => (
          <ActivityProjectionCard
            key={projection.sessionId}
            projection={projection}
            progress={progressBySession[projection.sessionId]}
            showProgress={showProgress}
            onPress={onPress}
            onResume={onResume}
          />
        ))}
      </View>
    </View>
  );
}
