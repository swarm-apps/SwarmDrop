/**
 * 媒体类型判定 —— file-browser 全链路唯一来源。
 *
 * 收敛自原先散落三处、互不一致的扩展名表(`file-icon.ts` 的 icon 分组、
 * `inbox/[itemId].tsx`、`inbox-list.tsx`),合并去重并补齐缺项
 * (heif / avif / tiff / wmv / flv / 3gp)。图标选择、缩略图判定、播放角标
 * 判定全部从这里读,避免再分叉。
 */

function extensionOf(name: string): string {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

/** 光栅图片扩展名(不含点)。svg 保留用于图标分组;缩略图渲染失败会回退图标。 */
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

export function isImageFile(name: string): boolean {
  return IMAGE_EXTENSIONS.has(extensionOf(name));
}

export function isVideoFile(name: string): boolean {
  return VIDEO_EXTENSIONS.has(extensionOf(name));
}
