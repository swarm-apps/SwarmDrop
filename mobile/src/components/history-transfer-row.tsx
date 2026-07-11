/**
 * 活动卡片 —— 在「传输活动」section 渲染一条 MobileTransferProjection。
 *
 * 与活跃卡片 [recent-transfer-row.tsx] 分开实现：数据形态不同
 * （history 含 finishedAt / errorMessage，active 含 speed / eta）。
 */

import { Trans } from "@lingui/react/macro";
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
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { projectionDirection, projectionStatus } from "@/core/transfer-types";

interface Props {
  item: MobileTransferProjection;
  onPress?: (sessionId: string) => void;
}

function HistoryTransferRowComponent({ item, onPress }: Props) {
  const direction = projectionDirection(item);
  const status = projectionStatus(item);
  const filesCount = item.files.length;
  const failedAndHasMessage = status === "failed" && !!item.errorMessage;
  const reason = projectionReasonLabel(item);

  return (
    <Pressable
      onPress={() => onPress?.(item.sessionId)}
      accessibilityRole="button"
      className="flex-row items-start gap-3 rounded-xl border border-border bg-card p-3 active:opacity-70"
    >
      <DirectionIcon direction={direction} />

      <View className="flex-1 gap-1">
        <View className="flex-row items-center justify-between gap-2">
          <Text
            className="flex-1 text-[14px] font-medium text-foreground"
            numberOfLines={1}
          >
            {item.peerName}
          </Text>
          <StatusBadge status={status} />
        </View>

        <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
          <Trans>{filesCount} 个文件</Trans>
          {" · "}
          {formatBytes(item.totalSize)}
          {" · "}
          {formatRelativeTime(item.startedAt)}
        </Text>

        {reason ? (
          <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
            {reason}
          </Text>
        ) : null}

        {failedAndHasMessage ? (
          <Text className="text-[12px] text-destructive-ink" numberOfLines={1}>
            <LocalizedError message={item.errorMessage} />
          </Text>
        ) : null}
      </View>
    </Pressable>
  );
}

export const HistoryTransferRow = memo(HistoryTransferRowComponent);
