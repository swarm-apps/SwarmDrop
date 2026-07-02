/**
 * FolderRow —— 文件树中的目录行（RN/NativeWind 版）。
 *
 * 与桌面端 `folder-row.tsx` 形态对齐：chevron + folder icon + name + 文件数。
 * select 模式下可移除整个目录（删除其下所有文件）。
 */

import { Trans } from "@lingui/react/macro";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
} from "lucide-react-native";
import { memo } from "react";
import { Pressable, useColorScheme, View } from "react-native";
import { formatBytes } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { RemoveButton } from "./file-tree-item";

/**
 * 文件夹图标色（Caution Amber）—— 是"文件夹"这一装饰性图形语义，而非 warning 状态，
 * 故不映射到 --warning token；但按 DESIGN.md 的 amber 亮/暗值随外观切换，暗色下提亮。
 */
const FOLDER_COLOR_LIGHT = "#F59E0B";
const FOLDER_COLOR_DARK = "#FACC15";

interface FolderRowProps {
  name: string;
  isExpanded: boolean;
  fileCount: number;
  totalSize: number;
  level?: number;
  mode: "select" | "transfer";
  onToggle: () => void;
  onRemove?: () => void;
}

function FolderRowComponent({
  name,
  isExpanded,
  fileCount,
  totalSize,
  level = 0,
  mode,
  onToggle,
  onRemove,
}: FolderRowProps) {
  const ChevronIcon = isExpanded ? ChevronDown : ChevronRight;
  const FolderIcon = isExpanded ? FolderOpen : Folder;
  const colors = useThemeColors();
  const folderColor =
    useColorScheme() === "dark" ? FOLDER_COLOR_DARK : FOLDER_COLOR_LIGHT;

  return (
    <Pressable
      onPress={onToggle}
      accessibilityRole="button"
      className={`flex-row items-center gap-2.5 rounded-lg py-2 pr-2 active:opacity-70 ${
        isExpanded ? "bg-accent/40" : ""
      }`}
      style={{ paddingLeft: level * 22 + 8 }}
    >
      <View className="min-w-0 flex-1 flex-row items-center gap-2.5">
        <ChevronIcon size={16} color={colors.mutedForeground} />
        <FolderIcon size={18} color={folderColor} />
        <Text
          className="min-w-0 flex-1 text-sm font-medium text-foreground"
          numberOfLines={1}
        >
          {name}
        </Text>
      </View>

      <View className="shrink-0 flex-row items-center gap-1.5">
        <Text className="text-xs text-muted-foreground">
          <Trans>{fileCount} 项</Trans>
          {mode === "select" && totalSize > 0
            ? ` · ${formatBytes(totalSize)}`
            : ""}
        </Text>
        {mode === "select" && onRemove ? (
          <RemoveButton onPress={onRemove} />
        ) : null}
      </View>
    </Pressable>
  );
}

export const FolderRow = memo(FolderRowComponent);
