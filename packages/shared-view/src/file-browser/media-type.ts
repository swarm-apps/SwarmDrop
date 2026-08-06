/**
 * 媒体类型判定 —— file-browser 全链路唯一来源。
 *
 * 从 `mobile/src/components/file-browser/media-type.ts` 整体上移。它在移动端就已经是
 * 「唯一来源」（收敛自原先散落三处、互不一致的扩展名表），只是那个「唯一」此前只在移动端
 * 成立——桌面的 `file-icon.ts` 与 Web 各有各的判断。上移之后三端共用同一张表。
 *
 * 图标选择、缩略图判定、播放角标判定全部从这里读。
 */

function extensionOf(name: string): string {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

/**
 * 光栅图片扩展名（不含点）。
 *
 * `svg` 与 `ico` 保留用于**图标分组**——它们当缩略图源未必理想，但渲染失败会自然回退到
 * 类型图标，而把它们排除在外会让图标分组少一档。
 */
export const IMAGE_EXTENSIONS: ReadonlySet<string> = new Set([
  "jpg",
  "jpeg",
  "png",
  "gif",
  "webp",
  "heic",
  "heif",
  "bmp",
  "tiff",
  "avif",
  "svg",
  "ico",
]);

export const VIDEO_EXTENSIONS: ReadonlySet<string> = new Set([
  "mp4",
  "mov",
  "m4v",
  "webm",
  "mkv",
  "avi",
  "wmv",
  "flv",
  "3gp",
]);

export const AUDIO_EXTENSIONS: ReadonlySet<string> = new Set([
  "mp3",
  "wav",
  "flac",
  "aac",
  "ogg",
  "wma",
  "m4a",
]);

export const ARCHIVE_EXTENSIONS: ReadonlySet<string> = new Set([
  "zip",
  "tar",
  "gz",
  "bz2",
  "xz",
  "rar",
  "7z",
]);

export const CODE_EXTENSIONS: ReadonlySet<string> = new Set([
  "ts",
  "tsx",
  "js",
  "jsx",
  "mjs",
  "cjs",
  "json",
  "css",
  "scss",
  "html",
  "rs",
  "py",
  "go",
  "java",
  "kt",
  "swift",
  "sh",
  "toml",
  "yaml",
  "yml",
]);

export const DOCUMENT_EXTENSIONS: ReadonlySet<string> = new Set([
  "md",
  "txt",
  "doc",
  "docx",
  "pdf",
  "rtf",
  "xls",
  "xlsx",
  "csv",
  "ppt",
  "pptx",
]);

export function isImageFile(name: string): boolean {
  return IMAGE_EXTENSIONS.has(extensionOf(name));
}

export function isVideoFile(name: string): boolean {
  return VIDEO_EXTENSIONS.has(extensionOf(name));
}

/** 文件大类。各端据此选图标与配色——**映射本身不进本包**，那是各端的表现层。 */
export type FileCategory =
  | "image"
  | "video"
  | "audio"
  | "archive"
  | "code"
  | "document"
  | "other";

/**
 * 扩展名 → 大类。
 *
 * 这是「扩展名表」在本仓的**唯一一份**。桌面此前在 `src/lib/file-icon.ts` 另有一份，
 * 与本表长期不一致（它有 `ico` 却缺 `heic`/`avif`/`tiff`，视频缺 `m4v`/`wmv`/`3gp`），
 * 于是同一个文件在「显示什么图标」与「要不要生成缩略图」两处会得到矛盾的答案。
 */
export function fileCategory(name: string): FileCategory {
  const ext = extensionOf(name);
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
  if (VIDEO_EXTENSIONS.has(ext)) return "video";
  if (AUDIO_EXTENSIONS.has(ext)) return "audio";
  if (ARCHIVE_EXTENSIONS.has(ext)) return "archive";
  if (CODE_EXTENSIONS.has(ext)) return "code";
  if (DOCUMENT_EXTENSIONS.has(ext)) return "document";
  return "other";
}
