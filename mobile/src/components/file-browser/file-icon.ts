import {
  FileArchive,
  FileCode,
  File as FileIcon,
  FileImage,
  FileText,
  FileVideo,
  type LucideIcon,
} from "lucide-react-native";

const ICON_GROUPS: ReadonlyArray<readonly [ReadonlySet<string>, LucideIcon]> = [
  [
    new Set(["jpg", "jpeg", "png", "gif", "svg", "webp", "bmp", "heic"]),
    FileImage,
  ],
  [new Set(["mp4", "mov", "m4v", "avi", "mkv", "webm"]), FileVideo],
  [new Set(["md", "txt", "doc", "docx", "pdf", "rtf"]), FileText],
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

export function fileBrowserIcon(name: string): LucideIcon {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  for (const [extensions, Icon] of ICON_GROUPS) {
    if (extensions.has(extension)) return Icon;
  }
  return FileIcon;
}
