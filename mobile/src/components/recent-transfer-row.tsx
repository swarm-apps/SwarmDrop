import { Trans } from "@lingui/react/macro";
import { Download, Upload } from "lucide-react-native";
import { Pressable, View } from "react-native";
import type {
  MobileTransferProgress,
  MobileTransferProjection,
} from "react-native-swarmdrop-core";
import {
  calcPercent,
  formatBytes,
  StatusBadge,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import {
  projectionDirection,
  projectionStatus,
  projectionTotalBytes,
  projectionTransferredBytes,
} from "@/core/transfer-types";

interface RecentTransferRowProps {
  projection: MobileTransferProjection;
  progress?: MobileTransferProgress;
  onPress?: (sessionId: string) => void;
}

/**
 * 最近传输行 —— 主屏单行,展示方向 / 进度 / 文件计数。
 */
export function RecentTransferRow({
  projection,
  progress,
  onPress,
}: RecentTransferRowProps) {
  const direction = projectionDirection(projection);
  const isOutgoing = direction === "send";
  const total = projectionTotalBytes(projection, progress);
  const transferred = projectionTransferredBytes(projection, progress);
  const percent = calcPercent(transferred, total);
  const status = projectionStatus(projection);

  return (
    <Pressable
      onPress={() => onPress?.(projection.sessionId)}
      accessibilityRole="button"
      className="flex-row items-center gap-3 rounded-xl border border-border bg-card p-3 active:opacity-70"
    >
      <View
        className={
          isOutgoing
            ? "size-9 items-center justify-center rounded-full bg-primary/10"
            : "size-9 items-center justify-center rounded-full bg-success/10"
        }
      >
        {isOutgoing ? (
          <Upload size={16} className="text-primary" />
        ) : (
          <Download size={16} className="text-success" />
        )}
      </View>

      <View className="flex-1 gap-1.5">
        <View className="flex-row items-center justify-between gap-2">
          <Text
            className="text-[13px] font-medium text-foreground"
            numberOfLines={1}
          >
            {isOutgoing ? <Trans>发送中</Trans> : <Trans>接收中</Trans>}
            {" · "}
            {projection.files.length} <Trans>文件</Trans>
          </Text>
          <StatusBadge status={status} />
        </View>
        <View className="h-1 overflow-hidden rounded-full bg-muted">
          <View
            className="h-full bg-primary"
            style={{ width: `${percent}%` }}
          />
        </View>
        <Text className="text-[11px] text-muted-foreground">
          {formatBytes(transferred)} / {formatBytes(total)}
        </Text>
      </View>
    </Pressable>
  );
}
