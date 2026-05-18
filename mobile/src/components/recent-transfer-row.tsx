import { Trans } from "@lingui/react/macro";
import { Download, Upload } from "lucide-react-native";
import { Pressable, View } from "react-native";
import type { MobileTransferProgress } from "react-native-swarmdrop-core";
import { Text } from "@/components/ui/text";

interface RecentTransferRowProps {
  snapshot: MobileTransferProgress;
  onPress?: (sessionId: string) => void;
}

/**
 * 最近传输行 —— 主屏单行,展示方向 / 进度 / 文件计数。
 */
export function RecentTransferRow({
  snapshot,
  onPress,
}: RecentTransferRowProps) {
  const isOutgoing = snapshot.direction === "send";
  const total = Number(snapshot.totalBytes);
  const transferred = Number(snapshot.transferredBytes);
  const percent =
    total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : 0;

  return (
    <Pressable
      onPress={() => onPress?.(snapshot.sessionId)}
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
            {snapshot.completedFiles.toString()}/
            {snapshot.totalFiles.toString()} <Trans>文件</Trans>
          </Text>
          <Text className="text-xs font-semibold text-muted-foreground">
            {percent}%
          </Text>
        </View>
        <View className="h-1 overflow-hidden rounded-full bg-muted">
          <View
            className="h-full bg-primary"
            style={{ width: `${percent}%` }}
          />
        </View>
      </View>
    </Pressable>
  );
}
