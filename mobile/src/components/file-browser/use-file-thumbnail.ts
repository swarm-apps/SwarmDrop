import { useEffect, useState } from "react";
import { isImageFile, isVideoFile } from "./media-type";
import type { FileBrowserItem } from "./types";
import { getVideoThumbnail } from "./video-thumbnail-cache";

/**
 * 会话级记忆:视频海报 uri 按 item.id 缓存,避免 cell 反复挂载/回收时重复触发生成。
 * 磁盘缓存是真正的存储层(见 video-thumbnail-cache),这里只是避免同会话内重复 IO。
 */
const videoThumbMemo = new Map<string, string>();

/**
 * 解析一个 file-browser item 的网格缩略图 uri。
 * - 无 `localUri` / 非 `file://` / 类型不支持 → undefined(调用方画类型图标)
 * - 图片 → 直接返回本地路径(由 expo-image 负责降采样),同步
 * - 视频 → 异步解析首帧海报,解析出来前返回 undefined(图标占位)
 */
export function useFileThumbnail(item: FileBrowserItem): string | undefined {
  const localUri = item.localUri;
  const usable = !!localUri && localUri.startsWith("file://");
  const isImage = usable && isImageFile(item.name);
  const isVideo = usable && isVideoFile(item.name);

  // 视频海报状态与 item.id 绑定。FlashList 回收会用同一个 cell 渲染不同 item,
  // 若不随 id 重置,复用 cell 会先闪出上一个视频的海报。用 React 官方「随 prop
  // 变化在渲染期重置 state」模式:id 变了立即重置(命中记忆则同步取回,否则 undefined)。
  const [video, setVideo] = useState<{ id: string; uri: string | undefined }>(
    () => ({
      id: item.id,
      uri: isVideo ? videoThumbMemo.get(item.id) : undefined,
    }),
  );
  if (video.id !== item.id) {
    setVideo({
      id: item.id,
      uri: isVideo ? videoThumbMemo.get(item.id) : undefined,
    });
  }

  useEffect(() => {
    if (!isVideo || !localUri || videoThumbMemo.has(item.id)) return;
    let cancelled = false;
    void getVideoThumbnail(localUri, item.id).then((uri) => {
      if (cancelled || !uri) return;
      videoThumbMemo.set(item.id, uri);
      // 仅当组件当前仍绑定同一 id 时写入(防止解析回来时 cell 已被复用给别的 item)。
      setVideo((prev) => (prev.id === item.id ? { id: item.id, uri } : prev));
    });
    return () => {
      cancelled = true;
    };
  }, [isVideo, localUri, item.id]);

  if (isImage) return localUri;
  // id 不匹配 = 渲染期重置尚未生效的那一帧,返回 undefined 而非上一个 item 的海报。
  if (isVideo) return video.id === item.id ? video.uri : undefined;
  return undefined;
}
