/**
 * 缩略图**契约** —— 判定与规格。
 *
 * **管线不在这里**：`createImageBitmap` / `OffscreenCanvas` 是 DOM API，进不了本包的
 * 门禁（`lib: ["ES2022"]`，无 DOM）。管线住在 `@swarmdrop/file-browser` 的
 * `use-thumbnail.ts`，取图源由各端注入。
 *
 * ## 适用范围：**走 JS 缩放管线的那条路**，不是字面意义的三端
 *
 * 这几条规格约束的是「拿到字节、自己解码、自己缩放」这条路径，目前只有 **Web** 走它
 * （`previewSource` → OPFS → `File` → 管线）。另两端都不经过：
 *
 * - **桌面**给的是 `previewUrl`（`convertFileSrc` 的 asset URL），资源协议自己流式读并
 *   降采样，`<img>` 直接渲染就行。
 * - **移动端**是 expo-image 原生解码，原生侧自带降采样与调度，且它还有视频海报——本契约
 *   只认图片，套上去反而会砍掉那条能力。
 *
 * 所以别把这里的常量当成「三端同一个数」去对齐移动端的原生参数，那是两套东西。
 */

import { isImageFile } from "./media-type";
import type { FileBrowserItem } from "./types";

/**
 * 缩略图长边上限（CSS 像素）。
 *
 * 320 足够撑满网格卡片的 4:3 预览区（各端卡宽都在 160–280 之间），再大只是浪费解码
 * 时间与缓存内存。一张 7.6 MB 的原图缩到这个尺寸后约 30 KB。
 */
export const THUMBNAIL_MAX_EDGE = 320;

/**
 * 超过这个大小的图**不生成缩略图**，直接给类型图标。
 *
 * 缩略图省的是「缓存驻留」，省不掉「解码峰值」——图片必须完整解码才能缩放，那一刻的
 * 内存占用就是原图的解码后尺寸。20 MB 的 JPEG 解码后可达数百 MB，一屏几张就能让
 * 浏览器标签页崩掉。给图标是诚实的降级：用户看不到预览，但列表还在。
 */
export const THUMBNAIL_MAX_SOURCE_BYTES = 20 * 1024 * 1024;

/**
 * 这个条目该不该**走缩放管线**生成缩略图。
 *
 * 三个条件缺一不可：是图片类型、有 `previewSource`、大小在门槛内。**缺失的文件一律不生成**
 * ——它已经不在盘上，取图只会白白失败一次（`fromInboxFiles` 也不会给它 `previewSource`，
 * 这里是第二道）。
 *
 * 判的是 `previewSource` 而不是 `previewUrl`：后者本来就能直接渲染，不经过管线，
 * 自然也没有解码峰值可言（见模块头的适用范围）。
 */
export function shouldGenerateThumbnail(item: FileBrowserItem): boolean {
  if (item.status === "missing") return false;
  if (!item.previewSource) return false;
  if (item.size > THUMBNAIL_MAX_SOURCE_BYTES) return false;
  return isImageFile(item.name);
}

/**
 * 缓存 key。
 *
 * 用 `previewSource` 而不是 `item.id`：同一个文件可能以不同的展示 ID 出现在不同场景
 * （发送侧是 `source:…`、收件箱是 `inbox:…`），但它们指向同一份字节，缩略图可以共用。
 * 带上 `size` 是为了让「同路径但内容换了」的情况自然失效。
 */
export function thumbnailCacheKey(item: FileBrowserItem): string | null {
  if (!item.previewSource) return null;
  return `${item.previewSource}#${item.size}`;
}

/**
 * 按长边上限算出目标尺寸，**不放大**小图。
 *
 * 返回的是整数像素——`OffscreenCanvas` 与原生解码器都接受小数，但小数宽高会让不同端
 * 因取整方向不同而产出差一像素的图，缓存也就不能跨端复用了。
 */
export function thumbnailTargetSize(
  width: number,
  height: number,
): { width: number; height: number } {
  if (width <= 0 || height <= 0) return { width: 0, height: 0 };
  const longest = Math.max(width, height);
  if (longest <= THUMBNAIL_MAX_EDGE) {
    return { width: Math.round(width), height: Math.round(height) };
  }
  const scale = THUMBNAIL_MAX_EDGE / longest;
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}
