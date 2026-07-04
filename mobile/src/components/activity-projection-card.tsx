import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react";
import { Trans } from "@lingui/react/macro";
import { AlertCircle, Inbox, RotateCcw } from "lucide-react-native";
import { memo } from "react";
import { Pressable, View } from "react-native";
import type {
  MobileTransferProgress,
  MobileTransferProjection,
} from "react-native-swarmdrop-core";
import {
  calcPercent,
  DirectionIcon,
  formatBytes,
  formatRelativeTime,
  LocalizedError,
  ProgressBar,
  projectionReasonLabel,
  StatusBadge,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import {
  projectionDirection,
  projectionPolicyNote,
  projectionStatus,
  projectionTotalBytes,
  projectionTransferredBytes,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";

interface ActivityProjectionCardProps {
  projection: MobileTransferProjection;
  progress?: MobileTransferProgress;
  /**
   * 是否显示进度条 —— 仅进行中(active)/可续传(recoverable)才为真。
   * 终态(已完成/已取消/已拒绝)与卡住(attention)不画进度条:进度条是"进行中"
   * 的语言,给终态配一根满格或半截的条会被误读成"仍在传输/暂停中"。
   */
  showProgress?: boolean;
  onPress: (sessionId: string) => void;
  onResume?: (sessionId: string) => void;
  /**
   * 反查命中的收件箱记录 id —— 仅"接收且已完成"且该会话已落库时由父级传入。
   * 用于在卡片尾部渲染「在收件箱查看」深链;未命中(冷启动 / 非接收 / 未落库)时缺席。
   */
  inboxItemId?: string;
  onOpenInbox?: (itemId: string) => void;
  /**
   * 状态徽章开关 —— 卡片住在与状态同名的分组下(「已完成」组/「正在进行」组)时,
   * 徽章只是复读分组标题,由父级关掉;混合状态的分组(需要注意/可恢复)保留。
   */
  showStatusBadge?: boolean;
}

function ActivityProjectionCardComponent({
  projection,
  progress,
  showProgress = false,
  onPress,
  onResume,
  inboxItemId,
  onOpenInbox,
  showStatusBadge = true,
}: ActivityProjectionCardProps) {
  // useLingui 一职两用:订阅 locale(policyNote 经全局 i18n 即时解析,memo 组件靠它
  // 在切换语言时重算)+ 提供 _ 翻译 a11y 文案。
  const { _ } = useLingui();
  const colors = useThemeColors();
  const direction = projectionDirection(projection);
  const status = projectionStatus(projection);
  const total = projectionTotalBytes(projection, progress);
  const transferred = projectionTransferredBytes(projection, progress);
  const percent = calcPercent(transferred, total);
  const reason = projectionReasonLabel(projection);
  // 已完成的卡不解释策略:自动接收成功是常规事实,灰条只会稀释「需要注意」组里
  // 真正需要解释的策略拒绝/待确认。
  const policyNote =
    status === "completed" ? null : projectionPolicyNote(projection);
  const canResume = projection.recoverable && onResume !== undefined;

  return (
    <Pressable
      onPress={() => onPress(projection.sessionId)}
      accessibilityRole="button"
      className="gap-3 rounded-lg border border-border bg-card p-3.5 active:opacity-70"
    >
      <View className="flex-row items-start gap-3">
        <DirectionIcon direction={direction} />
        <View className="min-w-0 flex-1 gap-1">
          <View className="flex-row items-center justify-between gap-2">
            <Text
              className="min-w-0 flex-1 text-[14px] font-semibold text-foreground"
              numberOfLines={1}
            >
              {direction === "send" ? (
                <Trans>发给 {projection.peerName}</Trans>
              ) : (
                <Trans>来自 {projection.peerName}</Trans>
              )}
            </Text>
            {showStatusBadge ? <StatusBadge status={status} /> : null}
          </View>
          <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
            {projection.files.length} <Trans>文件</Trans>
            {" · "}
            {formatBytes(total)}
            {" · "}
            {formatRelativeTime(projection.updatedAt)}
          </Text>
        </View>
        {inboxItemId && onOpenInbox ? (
          // 行尾快捷动作(与设备卡的发送按钮同一模式):跳到收件箱里对应的记录
          <Pressable
            onPress={(event) => {
              event.stopPropagation();
              onOpenInbox(inboxItemId);
            }}
            accessibilityRole="button"
            accessibilityLabel={_(msg`在收件箱查看`)}
            className="size-11 items-center justify-center self-center rounded-xl bg-muted active:opacity-70"
          >
            <Inbox color={colors.foreground} size={17} />
          </Pressable>
        ) : null}
      </View>

      {showProgress ? (
        <View className="gap-1.5">
          <ProgressBar percent={percent} heightClass="h-1.5" />
          <Text className="text-[11px] text-muted-foreground">
            {formatBytes(transferred)} / {formatBytes(total)}
          </Text>
        </View>
      ) : null}

      {reason || projection.errorMessage || policyNote ? (
        <View className="gap-1 rounded-lg bg-muted px-3 py-2">
          {reason ? (
            <Text className="text-[11px] text-muted-foreground">{reason}</Text>
          ) : null}
          {policyNote ? (
            <View className="flex-row items-center gap-1.5">
              <AlertCircle color={colors.mutedForeground} size={12} />
              <Text
                className="min-w-0 flex-1 text-[11px] text-muted-foreground"
                numberOfLines={2}
              >
                {policyNote}
              </Text>
            </View>
          ) : null}
          {projection.errorMessage ? (
            <Text
              className="text-[11px] text-destructive-ink"
              numberOfLines={2}
            >
              <LocalizedError message={projection.errorMessage} />
            </Text>
          ) : null}
        </View>
      ) : null}

      {canResume ? (
        <Pressable
          onPress={(event) => {
            event.stopPropagation();
            onResume?.(projection.sessionId);
          }}
          accessibilityRole="button"
          className="min-h-11 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70"
        >
          <RotateCcw color={colors.primaryForeground} size={15} />
          <Text className="text-[13px] font-semibold text-primary-foreground">
            <Trans>恢复传输</Trans>
          </Text>
        </Pressable>
      ) : null}
    </Pressable>
  );
}

/** 活动卡片:memo 让未变会话在每个 progress tick 跳过重渲染(progressBySession 只换更新会话的内层对象)。 */
export const ActivityProjectionCard = memo(ActivityProjectionCardComponent);
