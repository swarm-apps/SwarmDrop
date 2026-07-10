import { Trans, useLingui } from "@lingui/react/macro";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
  X,
} from "lucide-react-native";
import { memo, useCallback } from "react";
import { type GestureResponderEvent, Pressable, View } from "react-native";
import { formatBytes } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import type { FileBrowserActions } from "./types";

interface FolderRowProps {
  id: string;
  name: string;
  relativePath: string;
  depth: number;
  fileCount: number;
  size: bigint;
  expanded: boolean;
  actions?: FileBrowserActions;
  onToggle: (id: string) => void;
  testID?: string;
}

function FolderRowComponent({
  id,
  name,
  relativePath,
  depth,
  fileCount,
  size,
  expanded,
  actions,
  onToggle,
  testID,
}: FolderRowProps) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const ChevronIcon = expanded ? ChevronDown : ChevronRight;
  const FolderIcon = expanded ? FolderOpen : Folder;

  const toggle = useCallback(() => onToggle(id), [id, onToggle]);
  const remove = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.removeDirectory?.(relativePath);
    },
    [actions, relativePath],
  );

  return (
    <View
      className="min-h-14 flex-row items-center rounded-lg py-2 pr-2"
      style={{ paddingLeft: depth * 20 + 8 }}
    >
      <Pressable
        onPress={toggle}
        accessibilityRole="button"
        accessibilityState={{ expanded }}
        accessibilityLabel={t`${name} 文件夹，${fileCount} 项，${formatBytes(size)}`}
        testID={testID}
        className="min-h-11 min-w-0 flex-1 flex-row items-center gap-2.5 active:opacity-70"
      >
        <ChevronIcon size={16} color={colors.mutedForeground} />
        <FolderIcon size={18} color={colors.warning} />
        <View className="min-w-0 flex-1">
          <Text
            className="text-sm font-medium text-foreground"
            numberOfLines={1}
          >
            {name}
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            <Trans>{fileCount} 项</Trans> · {formatBytes(size)}
          </Text>
        </View>
      </Pressable>
      {actions?.removeDirectory ? (
        <Pressable
          onPress={remove}
          accessibilityRole="button"
          accessibilityLabel={t`移除文件夹 ${name}`}
          className="size-11 items-center justify-center rounded-xl active:bg-destructive/15"
        >
          <X size={16} color={colors.mutedForeground} />
        </Pressable>
      ) : null}
    </View>
  );
}

export const FolderRow = memo(FolderRowComponent);
