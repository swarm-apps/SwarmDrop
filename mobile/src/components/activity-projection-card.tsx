import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react";
import { Trans } from "@lingui/react/macro";
import { AlertCircle, Inbox, RotateCcw } from "lucide-react-native";
import { memo } from "react";
import { Pressable, View } from "react-native";
import type { MobileTransferProjection } from "react-native-swarmdrop-core";
import {
  DirectionIcon,
  formatBytes,
  formatRelativeTime,
  LocalizedError,
  projectionReasonLabel,
  StatusBadge,
  TransferProgressReadout,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import {
  isProjectionRecoverable,
  projectionDirection,
  projectionPolicyNote,
  projectionStatus,
  projectionTotalBytes,
  projectionTransferredBytes,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import type { ProgressFrame, PublishingFile } from "@/stores/transfer-store";

interface ActivityProjectionCardProps {
  projection: MobileTransferProjection;
  progress?: ProgressFrame;
  /**
   * 该会话此刻正在保存的文件（暂存 → 用户可见位置）。在场时进度块换成发布态：
   * 字节已经收完，但 Android 的 SAF 目标还要整份拷一遍，几十秒起步。
   */
  publishing?: PublishingFile;
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
}

function ActivityProjectionCardComponent({
  projection,
  progress,
  publishing,
  showProgress = false,
  onPress,
  onResume,
  inboxItemId,
  onOpenInbox,
}: ActivityProjectionCardProps) {
  // useLingui 一职两用:订阅 locale(policyNote 经全局 i18n 即时解析,memo 组件靠它
  // 在切换语言时重算)+ 提供 _ 翻译 a11y 文案。
  const { _ } = useLingui();
  const colors = useThemeColors();
  const direction = projectionDirection(projection);
  const status = projectionStatus(projection);
  const total = projectionTotalBytes(projection, progress);
  const transferred = projectionTransferredBytes(projection, progress);
  const reason = projectionReasonLabel(projection);
  // 已完成的卡不解释策略:自动接收成功是常规事实,灰条只会稀释「需要注意」组里
  // 真正需要解释的策略拒绝/待确认。
  const policyNote =
    status === "completed" ? null : projectionPolicyNote(projection);
  // 用窄谓词(phase===Suspended && recoverable),不用裸 recoverable —— 即便日后别的
  // section 也传 onResume,offered/waiting 卡片也不会误显「恢复传输」。
  const canResume =
    isProjectionRecoverable(projection) && onResume !== undefined;

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
            {/* 徽章恒显：列表是纯时间线，没有分组标题替这一行说明状态
                （DESIGN.md 的 Transfer List Order Contract）。 */}
            <StatusBadge status={status} />
          </View>
          <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
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

      {/* 活动页**没有会话级横幅**，这张卡就是发布态的唯一表面 —— 契约允许次要表面用
          不点名的「正在保存…」正是因为「会话级横幅已经说清是哪个了」，这里没有。
          `surface="card"` 那一档因此保留点名与保存百分比。 */}
      {showProgress ? (
        <TransferProgressReadout
          surface="card"
          transferred={transferred}
          total={total}
          status={status}
          progress={progress}
          publishing={publishing}
        />
      ) : null}

      {reason || projection.failure || policyNote ? (
        <View className="gap-1 rounded-lg bg-muted px-3 py-2">
          {reason ? (
            <Text className="text-[12px] text-muted-foreground">{reason}</Text>
          ) : null}
          {policyNote ? (
            <View className="flex-row items-center gap-1.5">
              <AlertCircle color={colors.mutedForeground} size={12} />
              <Text
                className="min-w-0 flex-1 text-[12px] text-muted-foreground"
                numberOfLines={2}
              >
                {policyNote}
              </Text>
            </View>
          ) : null}
          {projection.failure ? (
            <Text
              className="text-[12px] text-destructive-ink"
              numberOfLines={2}
            >
              <LocalizedError failure={projection.failure} />
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

/**
 * 传输会话列表的 `keyExtractor` —— 与卡片同住:它是**这张卡片在列表里的呈现属性**,
 * 不是某个屏的私有细节(传输记录页与传输搜索页此前各抄了一份)。模块级常量,引用稳定。
 *
 * 行间距不在这里,在 `mobile/screen.tsx` 的 `ListItemGap` —— 那个没有任何 transfer 语义,
 * 放这儿会挡住第三个调用点(`send/share-target` 就曾因此又抄了一份)。
 */
export const transferSessionKey = (item: MobileTransferProjection) =>
  item.sessionId;
