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
            className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70"
          >
            <Trash2 color={colors.mutedForeground} size={19} />
          </Pressable>
        }
      />

      <Text className="px-1 text-[12px] text-muted-foreground">
        <Trans>传输过程、恢复入口和诊断记录</Trans>
      </Text>

      {hasContent ? (
        <>
          <ProjectionSection
            title={<Trans>正在进行</Trans>}
            empty={<Trans>没有正在传输的项目</Trans>}
            projections={grouped.active}
            progressBySession={progressBySession}
            onPress={goDetail}
          />
          <ProjectionSection
            title={<Trans>可恢复</Trans>}
            empty={<Trans>没有等待恢复的传输</Trans>}
            projections={grouped.recoverable}
            progressBySession={progressBySession}
            onPress={goDetail}
            onResume={resume}
          />
          <ProjectionSection
            title={<Trans>需要注意</Trans>}
            empty={<Trans>没有需要处理的问题</Trans>}
            projections={grouped.attention}
            progressBySession={progressBySession}
            onPress={goDetail}
          />
          <ProjectionSection
            title={<Trans>完成诊断</Trans>}
            empty={<Trans>完成和取消记录会出现在这里</Trans>}
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
      />
    </AppScreen>
  );
}

function ProjectionSection({
  title,
  empty,
  projections,
  progressBySession,
  onPress,
  onResume,
}: {
  title: React.ReactNode;
  empty: React.ReactNode;
  projections: MobileTransferProjection[];
  progressBySession: Record<string, MobileTransferProgress>;
  onPress: (sessionId: string) => void;
  onResume?: (sessionId: string) => void;
}) {
  return (
    <View className="gap-2.5">
      <Text className="text-[15px] font-semibold text-foreground">{title}</Text>
      {projections.length > 0 ? (
        <View className="gap-2">
          {projections.map((projection) => (
            <ActivityProjectionCard
              key={projection.sessionId}
              projection={projection}
              progress={progressBySession[projection.sessionId]}
              onPress={onPress}
              onResume={onResume}
            />
          ))}
        </View>
      ) : (
        <View className="rounded-lg border border-border bg-card px-3.5 py-4">
          <Text className="text-center text-[12px] text-muted-foreground">
            {empty}
          </Text>
        </View>
      )}
    </View>
  );
}
