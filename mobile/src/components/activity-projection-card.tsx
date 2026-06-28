import { Trans } from "@lingui/react/macro";
import { AlertCircle, RotateCcw } from "lucide-react-native";
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
  onPress: (sessionId: string) => void;
  onResume?: (sessionId: string) => void;
}

export function ActivityProjectionCard({
  projection,
  progress,
  onPress,
  onResume,
}: ActivityProjectionCardProps) {
  const colors = useThemeColors();
  const direction = projectionDirection(projection);
  const status = projectionStatus(projection);
  const total = projectionTotalBytes(projection, progress);
  const transferred = projectionTransferredBytes(projection, progress);
  const percent = calcPercent(transferred, total);
  const reason = projectionReasonLabel(projection);
  const policyNote = projectionPolicyNote(projection);
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
              {projection.peerName}
            </Text>
            <StatusBadge status={status} />
          </View>
          <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
            {projection.files.length} <Trans>文件</Trans>
            {" · "}
            {formatBytes(total)}
            {" · "}
            {formatRelativeTime(projection.updatedAt)}
          </Text>
        </View>
      </View>

      <View className="gap-1.5">
        <ProgressBar percent={percent} heightClass="h-1.5" />
        <Text className="text-[11px] text-muted-foreground">
          {formatBytes(transferred)} / {formatBytes(total)}
        </Text>
      </View>

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
            <Text className="text-[11px] text-destructive" numberOfLines={2}>
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
          <RotateCcw color={colors.background} size={15} />
          <Text className="text-[13px] font-semibold text-primary-foreground">
            <Trans>恢复传输</Trans>
          </Text>
        </Pressable>
      ) : null}
    </Pressable>
  );
}
