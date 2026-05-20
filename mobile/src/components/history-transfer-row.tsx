/**
 * 历史卡片 —— 在「传输历史」section 渲染一条 MobileTransferHistoryItem。
 *
 * 与活跃卡片 [recent-transfer-row.tsx] 分开实现：数据形态不同
 * （history 含 finishedAt / errorMessage，active 含 speed / eta）。
 */

import { Trans } from "@lingui/react/macro";
import { Pressable, View } from "react-native";
import {
  MobileSessionStatus,
  type MobileTransferHistoryItem,
} from "react-native-swarmdrop-core";
import {
  DirectionIcon,
  formatBytes,
  formatRelativeTime,
  LocalizedError,
  StatusBadge,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";

interface Props {
  item: MobileTransferHistoryItem;
  onPress?: (sessionId: string) => void;
}

export function HistoryTransferRow({ item, onPress }: Props) {
  const direction = item.direction === "send" ? "send" : "receive";
  const filesCount = item.files.length;
  const failedAndHasMessage =
    item.status === MobileSessionStatus.Failed && !!item.errorMessage;

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
          <StatusBadge status={item.status} />
        </View>

        <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
          <Trans>{filesCount} 个文件</Trans>
          {" · "}
          {formatBytes(item.totalSize)}
          {" · "}
          {formatRelativeTime(item.startedAt)}
        </Text>

        {failedAndHasMessage ? (
          <Text className="text-[11px] text-destructive" numberOfLines={1}>
            <LocalizedError message={item.errorMessage} />
          </Text>
        ) : null}
      </View>
    </Pressable>
  );
}
