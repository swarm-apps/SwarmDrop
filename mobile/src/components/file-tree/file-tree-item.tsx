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
import { cn } from "@/lib/utils";
import type { FileStatus } from "./data";

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
  return (
    <Pressable
      onPress={onPress}
      hitSlop={6}
      accessibilityRole="button"
      className="rounded-full p-1 active:bg-destructive/15"
    >
      <X size={13} className="text-muted-foreground" />
    </Pressable>
  );
}

/* ─── 变体样式 ─── */

const variantStyles: Record<
  FileStatus,
  { row: string; iconColor: string; nameColor: string; infoColor: string }
> = {
  select: {
    row: "",
    iconColor: "text-primary",
    nameColor: "text-foreground",
    infoColor: "text-muted-foreground",
  },
  waiting: {
    row: "opacity-60",
    iconColor: "text-muted-foreground",
    nameColor: "text-muted-foreground",
    infoColor: "text-muted-foreground",
  },
  transferring: {
    row: "bg-blue-500/10",
    iconColor: "text-blue-500",
    nameColor: "text-foreground",
    infoColor: "text-blue-500",
  },
  completed: {
    row: "bg-success/5",
    iconColor: "text-success",
    nameColor: "text-foreground",
    infoColor: "text-muted-foreground",
  },
  error: {
    row: "bg-destructive/5",
    iconColor: "text-destructive",
    nameColor: "text-foreground",
    infoColor: "text-destructive",
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
  const Icon = getFileIcon(name);
  const isTransferring = variant === "transferring";
  const isPressable = onPress != null || onLongPress != null;

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
          <Icon size={18} className={cn("shrink-0", styles.iconColor)} />
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
              <RotateCcw size={13} className="text-destructive" />
            </Pressable>
          ) : null}
          {variant === "completed" ? (
            <Check size={13} className="text-success" />
          ) : null}
          {variant === "waiting" ? (
            <Timer size={13} className="text-muted-foreground" />
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
