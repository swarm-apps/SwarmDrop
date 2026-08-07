/**
 * 文件类型图标 + 配色映射。
 *
 * **分类判定不在这里**——那是 `@swarmdrop/shared-view` 的 `fileCategory()`，三端同一份
 * 扩展名表。本文件只做「大类 → Lucide 图标 + tailwind 配色类」这一层，因为 Lucide 是
 * React DOM 的东西，移动端有自己的图标集。
 *
 * 迁自桌面 `src/lib/file-icon.ts`。那一版自带一张扩展名表，与 `media-type` 长期不一致
 * （有 `ico` 却缺 `heic`/`avif`/`tiff`，视频缺 `m4v`/`wmv`/`3gp`），于是同一个文件在
 * 「显示什么图标」与「要不要生成缩略图」两处会得到矛盾的答案。现在两处同源。
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
import { fileCategory, type FileCategory } from "@swarmdrop/shared-view";

const CATEGORY_STYLE: Record<FileCategory, { icon: LucideIcon; color: string }> = {
  image: { icon: FileImage, color: "text-green-500" },
  video: { icon: FileVideo, color: "text-purple-500" },
  audio: { icon: FileAudio, color: "text-pink-500" },
  archive: { icon: FileArchive, color: "text-amber-500" },
  code: { icon: FileCode, color: "text-muted-foreground" },
  document: { icon: FileText, color: "text-blue-500" },
  other: { icon: File, color: "text-muted-foreground" },
};

/**
 * 文件名后缀 → 图标 + 配色（无匹配回退到通用 File / muted-foreground）。
 *
 * **一次调用给两样**，别拆成 `getFileIcon` + `getFileIconColor` 各调一次：那样每行每次
 * 渲染要跑两遍 `fileCategory()`（切扩展名 + 最多六次 Set 查找），而这批行在传输中每秒
 * 都在重画。返回的是模块级常量对象，不产生新分配。
 */
export function getFileIconStyle(name: string): {
  icon: LucideIcon;
  color: string;
} {
  return CATEGORY_STYLE[fileCategory(name)];
}
