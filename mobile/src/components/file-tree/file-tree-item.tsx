/**
 * FileTreeItem —— 文件树中的文件行（RN/NativeWind 版）。
 *
 * 与桌面端 file-tree-item.tsx 形态对齐：图标 + 文件名 + 信息区，
 * 5 种状态变体（select / waiting / transferring / completed / error）。
 */

import { Trans } from "@lingui/react/macro";
import {
  Check,
  FileArchive,
  FileCode,
  File as FileIcon,
  FileImage,
  FileText,
  type LucideIcon,
  RotateCcw,
  Timer,
  X,
} from "lucide-react-native";
import { Pressable, View } from "react-native";
import { formatBytes, ProgressBar } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";
import type { FileStatus } from "./data";

/** transferring 蓝色（blue-500）固定值，与桌面端一致 */
const TRANSFERRING_COLOR = "#3b82f6";

interface FileTreeItemProps {
  name: string;
  size: number;
  variant: FileStatus;
  /** 0-100，仅 transferring 用 */
  progress?: number;
  level?: number;
  /** select 模式下点 X 删除 */
  onRemove?: () => void;
  /** error 模式下点重试 */
  onRetry?: () => void;
  /** 整行点击（如详情页的「分享文件」），不传则整行不可点击 */
  onPress?: () => void;
  /** 整行长按（如详情页的「复制路径」） */
  onLongPress?: () => void;
}

/* ─── 文件图标映射 ─── */

const EXT_ICON_MAP: [ReadonlySet<string>, LucideIcon][] = [
  [
    new Set(["jpg", "jpeg", "png", "gif", "svg", "webp", "bmp", "ico", "heic"]),
    FileImage,
  ],
  [new Set(["md", "txt", "doc", "docx", "pdf"]), FileText],
  [
    new Set([
      "ts",
      "tsx",
      "js",
      "jsx",
      "json",
      "css",
      "html",
      "rs",
      "py",
      "go",
      "java",
      "toml",
      "yaml",
      "yml",
      "sh",
      "swift",
      "kt",
    ]),
    FileCode,
  ],
  [new Set(["zip", "tar", "gz", "rar", "7z"]), FileArchive],
];

function getFileIcon(name: string): LucideIcon {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  for (const [exts, Icon] of EXT_ICON_MAP) {
    if (exts.has(ext)) return Icon;
  }
  return FileIcon;
}

/* ─── 共享移除按钮 ─── */

export function RemoveButton({ onPress }: { onPress?: () => void }) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      hitSlop={6}
      accessibilityRole="button"
      className="rounded-full p-1 active:bg-destructive/15"
    >
      <X size={13} color={colors.mutedForeground} />
    </Pressable>
  );
}

/**
 * 变体样式表。`iconColor` 用「解析为颜色字符串」的回调，避免和外面
 * 的 className 写法分裂成两套来源；新增 variant 只动这一处。
 */
type ThemeColors = ReturnType<typeof useThemeColors>;
type VariantStyle = {
  row: string;
  nameColor: string;
  infoColor: string;
  iconColor: (c: ThemeColors) => string;
};

const variantStyles: Record<FileStatus, VariantStyle> = {
  select: {
    row: "",
    nameColor: "text-foreground",
    infoColor: "text-muted-foreground",
    iconColor: (c) => c.primary,
  },
  waiting: {
    row: "opacity-60",
    nameColor: "text-muted-foreground",
    infoColor: "text-muted-foreground",
    iconColor: (c) => c.mutedForeground,
  },
  transferring: {
    row: "bg-blue-500/10",
    nameColor: "text-foreground",
    infoColor: "text-blue-500",
    iconColor: () => TRANSFERRING_COLOR,
  },
  completed: {
    row: "bg-success/5",
    nameColor: "text-foreground",
    infoColor: "text-muted-foreground",
    iconColor: (c) => c.success,
  },
  error: {
    row: "bg-destructive/5",
    nameColor: "text-foreground",
    infoColor: "text-destructive",
    iconColor: (c) => c.destructive,
  },
};

/* ─── 主组件 ─── */

export function FileTreeItem({
  name,
  size,
  variant,
  progress = 0,
  level = 0,
  onRemove,
  onRetry,
  onPress,
  onLongPress,
}: FileTreeItemProps) {
  const styles = variantStyles[variant];
  const colors = useThemeColors();
  const Icon = getFileIcon(name);
  const isTransferring = variant === "transferring";
  const isPressable = onPress != null || onLongPress != null;
  const iconColor = styles.iconColor(colors);

  return (
    <Pressable
      onPress={onPress}
      onLongPress={onLongPress}
      // 不传 onPress / onLongPress 时 Pressable 不消耗触摸事件
      accessibilityRole={isPressable ? "button" : undefined}
      className={cn(
        "flex-col rounded-lg py-2 pr-2",
        styles.row,
        isPressable && "active:opacity-70",
      )}
      style={{ paddingLeft: level * 22 + 8, gap: isTransferring ? 6 : 0 }}
    >
      <View className="flex-row items-center gap-2.5">
        <View className="min-w-0 flex-1 flex-row items-center gap-2.5">
          <Icon size={18} color={iconColor} />
          <Text
            className={cn("min-w-0 flex-1 text-sm", styles.nameColor)}
            numberOfLines={1}
          >
            {name}
          </Text>
        </View>

        <View className="shrink-0 flex-row items-center gap-1.5">
          <Text className={cn("text-xs", styles.infoColor)}>
            {isTransferring ? (
              `${Math.round(progress)}%`
            ) : variant === "error" ? (
              <Trans>失败</Trans>
            ) : (
              formatBytes(size)
            )}
          </Text>
          {variant === "select" && onRemove ? (
            <RemoveButton onPress={onRemove} />
          ) : null}
          {variant === "error" && onRetry ? (
            <Pressable
              onPress={onRetry}
              hitSlop={6}
              accessibilityRole="button"
              className="rounded-full p-1 active:bg-destructive/15"
            >
              <RotateCcw size={13} color={colors.destructive} />
            </Pressable>
          ) : null}
          {variant === "completed" ? (
            <Check size={13} color={colors.success} />
          ) : null}
          {variant === "waiting" ? (
            <Timer size={13} color={colors.mutedForeground} />
          ) : null}
        </View>
      </View>

      {isTransferring ? (
        <ProgressBar
          percent={progress}
          heightClass="h-1"
          fillClass="bg-blue-500"
        />
      ) : null}
    </Pressable>
  );
}
