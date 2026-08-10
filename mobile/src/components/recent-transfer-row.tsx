import { Trans } from "@lingui/react/macro";
import { ArrowDownToLine, ArrowUpFromLine } from "lucide-react-native";
import { memo } from "react";
import { Pressable, View } from "react-native";
import type { MobileTransferProjection } from "react-native-swarmdrop-core";
import {
  StatusBadge,
  TransferProgressReadout,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import {
  projectionDirection,
  projectionStatus,
  projectionTotalBytes,
  projectionTransferredBytes,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";
import type { ProgressFrame, PublishingFile } from "@/stores/transfer-store";

interface RecentTransferRowProps {
  projection: MobileTransferProjection;
  progress?: ProgressFrame;
  /** 该会话此刻正在保存的文件；在场时这一行改说「正在保存 {文件名}」。 */
  publishing?: PublishingFile;
  onPress?: (sessionId: string) => void;
}

/**
 * 最近传输行 —— 主屏单行,展示方向 / 进度 / 文件计数。
 */
function RecentTransferRowComponent({
  projection,
  progress,
  publishing,
  onPress,
}: RecentTransferRowProps) {
  const direction = projectionDirection(projection);
  const isOutgoing = direction === "send";
  const total = projectionTotalBytes(projection, progress);
  const transferred = projectionTransferredBytes(projection, progress);
  const status = projectionStatus(projection);
  const colors = useThemeColors();

  return (
    <Pressable
      onPress={() => onPress?.(projection.sessionId)}
      accessibilityRole="button"
      className="flex-row items-center gap-3 rounded-xl border border-border bg-card p-3 active:opacity-70"
    >
      <View
        className={cn(
          "size-9 items-center justify-center rounded-full",
          isOutgoing ? "bg-primary/10" : "bg-success/10",
        )}
      >
        {isOutgoing ? (
          <ArrowUpFromLine size={16} color={colors.primary} />
        ) : (
          <ArrowDownToLine size={16} color={colors.success} />
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
        {/* 共享原语而不是手绘的 h-1 View —— 后者绕过了 tone 查表，是「阶段收口为具名
            tone」那一轮的漏网点：发布期该变灰的地方它永远是品牌青绿。 */}
        <TransferProgressReadout
          surface="row"
          transferred={transferred}
          total={total}
          status={status}
          progress={progress}
          publishing={publishing}
        />
      </View>
    </Pressable>
  );
}

/** 高频进度行:memo 让未变会话在每个 progress tick 跳过重渲染。 */
export const RecentTransferRow = memo(RecentTransferRowComponent);
