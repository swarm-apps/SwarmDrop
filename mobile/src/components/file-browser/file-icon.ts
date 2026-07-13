import {
  FileArchive,
  FileCode,
  File as FileIcon,
  FileImage,
  FileText,
  FileVideo,
  type LucideIcon,
} from "lucide-react-native";
import { IMAGE_EXTENSIONS, VIDEO_EXTENSIONS } from "./media-type";

const TEXT_EXTENSIONS = new Set(["md", "txt", "doc", "docx", "pdf", "rtf"]);
const CODE_EXTENSIONS = new Set([
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
]);
const ARCHIVE_EXTENSIONS = new Set(["zip", "tar", "gz", "rar", "7z"]);

// 图片/视频扩展名从 media-type 单一来源引入,避免与缩略图判定分叉。
const ICON_GROUPS: ReadonlyArray<readonly [ReadonlySet<string>, LucideIcon]> = [
  [IMAGE_EXTENSIONS, FileImage],
  [VIDEO_EXTENSIONS, FileVideo],
  [TEXT_EXTENSIONS, FileText],
  [CODE_EXTENSIONS, FileCode],
  [ARCHIVE_EXTENSIONS, FileArchive],
];

export function fileBrowserIcon(name: string): LucideIcon {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  for (const [extensions, Icon] of ICON_GROUPS) {
    if (extensions.has(extension)) return Icon;
  }
  return FileIcon;
}
