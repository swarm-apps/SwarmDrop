/**
 * 文件类型图标 + 配色映射（单一来源）
 *
 * 按扩展名返回对应的 Lucide 图标组件与 tailwind 配色类。调用方控制 size 与多文件
 * 聚合（count）等渲染细节，这里负责「扩展名 → 图标 + 配色」这一份映射，让 file-tree /
 * history-item / inbox 三处共用同一套字形与配色（之前各搓一份、字形与颜色都漂移）。
 *
 * 注：file-tree 行按传输状态着色，只取 [`getFileIcon`] 不用 [`getFileIconColor`]。
 */

import {
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileText,
  FileVideo,
  type LucideIcon,
} from "lucide-react";

interface FileType {
  exts: ReadonlySet<string>;
  icon: LucideIcon;
  /** tailwind text-* 配色类（history-item / inbox 用）。 */
  color: string;
}

const FILE_TYPES: readonly FileType[] = [
  {
    exts: new Set(["jpg", "jpeg", "png", "gif", "svg", "webp", "bmp", "ico"]),
    icon: FileImage,
    color: "text-green-500",
  },
  {
    exts: new Set(["mp4", "mov", "avi", "mkv", "webm", "flv"]),
    icon: FileVideo,
    color: "text-purple-500",
  },
  {
    exts: new Set(["mp3", "wav", "flac", "aac", "ogg", "wma"]),
    icon: FileAudio,
    color: "text-pink-500",
  },
  {
    exts: new Set(["zip", "tar", "gz", "bz2", "rar", "7z"]),
    icon: FileArchive,
    color: "text-amber-500",
  },
  {
    exts: new Set([
      "ts", "tsx", "js", "jsx", "json", "css", "html",
      "rs", "py", "go", "java", "toml", "yaml", "yml",
    ]),
    icon: FileCode,
    color: "text-muted-foreground",
  },
  {
    exts: new Set([
      "md", "txt", "doc", "docx", "pdf",
      "xls", "xlsx", "ppt", "pptx",
    ]),
    icon: FileText,
    color: "text-blue-500",
  },
];

const DEFAULT_TYPE = { icon: File, color: "text-muted-foreground" } as const;

function lookup(name: string): { icon: LucideIcon; color: string } {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return FILE_TYPES.find((t) => t.exts.has(ext)) ?? DEFAULT_TYPE;
}

/** 根据文件名后缀返回 Lucide 图标组件（无匹配回退到通用 File）。 */
export function getFileIcon(name: string): LucideIcon {
  return lookup(name).icon;
}

/** 根据文件名后缀返回 tailwind 配色类（无匹配回退到 muted-foreground）。 */
export function getFileIconColor(name: string): string {
  return lookup(name).color;
}
