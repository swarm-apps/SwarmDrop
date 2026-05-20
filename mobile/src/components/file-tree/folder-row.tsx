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
import { Pressable, View } from "react-native";
import { formatBytes } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { RemoveButton } from "./file-tree-item";

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

export function FolderRow({
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
        <ChevronIcon size={16} className="shrink-0 text-muted-foreground" />
        <FolderIcon size={18} className="shrink-0 text-amber-500" />
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
