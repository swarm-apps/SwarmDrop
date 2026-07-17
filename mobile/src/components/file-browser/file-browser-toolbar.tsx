import { Trans, useLingui } from "@lingui/react/macro";
import { Grid2X2, ListTree } from "lucide-react-native";
import { memo } from "react";
import { Pressable, View } from "react-native";
import { formatBytes } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";
import type { FileBrowserView } from "./types";

interface FileBrowserToolbarProps {
  title?: React.ReactElement | string;
  count: number;
  totalSize: bigint;
  view: FileBrowserView;
  onViewChange: (view: FileBrowserView) => void;
  testID?: string;
}

function FileBrowserToolbarComponent({
  title,
  count,
  totalSize,
  view,
  onViewChange,
  testID,
}: FileBrowserToolbarProps) {
  const { t } = useLingui();
  const colors = useThemeColors();
  return (
    <View
      className="flex-row items-center justify-between gap-3 bg-background py-3"
      testID={testID}
    >
      <View className="min-w-0 flex-1">
        <Text className="text-[13px] font-semibold text-foreground">
          {title ?? <Trans>文件</Trans>}
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          <Trans>{count} 项</Trans> · {formatBytes(totalSize)}
        </Text>
      </View>
      <View className="flex-row rounded-xl border border-border bg-card p-1">
        <Pressable
          onPress={() => onViewChange("tree")}
          accessibilityRole="button"
          accessibilityLabel={t`树形视图`}
          accessibilityState={{ selected: view === "tree" }}
          testID={`${testID ?? "file-browser"}-view-tree`}
          className={cn(
            "size-13 items-center justify-center rounded-lg",
            view === "tree" ? "bg-primary/15" : "active:bg-muted",
          )}
        >
          <ListTree
            size={18}
            color={view === "tree" ? colors.primary : colors.mutedForeground}
          />
        </Pressable>
        <Pressable
          onPress={() => onViewChange("grid")}
          accessibilityRole="button"
          accessibilityLabel={t`网格视图`}
          accessibilityState={{ selected: view === "grid" }}
          testID={`${testID ?? "file-browser"}-view-grid`}
          className={cn(
            "size-13 items-center justify-center rounded-lg",
            view === "grid" ? "bg-primary/15" : "active:bg-muted",
          )}
        >
          <Grid2X2
            size={18}
            color={view === "grid" ? colors.primary : colors.mutedForeground}
          />
        </Pressable>
      </View>
    </View>
  );
}

export const FileBrowserToolbar = memo(FileBrowserToolbarComponent);
