/**
 * 视频首帧海报生成 + 磁盘缓存。
 *
 * `getThumbnailAsync` 每次调用都会往 cache 写一个新临时文件并返回瞬时路径,
 * 直接用会让缓存无界增长。这里按稳定 id keying,把结果 move 到确定性路径
 * `<cache>/video-thumbs/<key>.jpg`,`dest.exists` 就短路(零解码);并发上限
 * 防止快速滚动时的解码风暴;失败返回 null,调用方回退类型图标。
 */

import { Directory, File, Paths } from "expo-file-system";
import * as VideoThumbnails from "expo-video-thumbnails";

const THUMB_DIR = new Directory(Paths.cache, "video-thumbs");
const MAX_CONCURRENT = 3;

let active = 0;
const waiters: Array<() => void> = [];

function acquire(): Promise<void> {
  if (active < MAX_CONCURRENT) {
    active += 1;
    return Promise.resolve();
  }
  // 排队者不自增 active;由 release 把「持有」直接移交(见下),保证 active 始终 = 实际持有数。
  return new Promise<void>((resolve) => waiters.push(resolve));
}

function release(): void {
  const next = waiters.shift();
  if (next) {
    next(); // 名额同步移交给下一个排队者,active 不变(仍占用),不会瞬时超配
  } else {
    active -= 1;
  }
}

/**
 * 把任意稳定 id 压成安全短文件名。双哈希(djb2 + sdbm)拼成 ~64bit base36 键,
 * 既避开 id 里的 `:`/`/` 等非法字符,又把碰撞概率压到可忽略(单 32bit 哈希碰撞会静默串图)。
 */
function cacheKey(id: string): string {
  let h1 = 5381;
  let h2 = 0;
  for (let i = 0; i < id.length; i += 1) {
    const c = id.charCodeAt(i);
    h1 = ((h1 << 5) + h1 + c) | 0; // djb2
    h2 = (c + (h2 << 6) + (h2 << 16) - h2) | 0; // sdbm
  }
  return (h1 >>> 0).toString(36) + (h2 >>> 0).toString(36);
}

/**
 * 取/生成视频海报,返回缓存的 `file://` uri;任何失败(编解码不支持、无帧等)返回 null。
 * @param localVideoUri 本地视频 `file://` 路径
 * @param id 该文件的稳定 id(用于缓存 keying)
 */
export async function getVideoThumbnail(
  localVideoUri: string,
  id: string,
): Promise<string | null> {
  try {
    if (!THUMB_DIR.exists) {
      THUMB_DIR.create({ intermediates: true, idempotent: true });
    }
    const dest = new File(THUMB_DIR, `${cacheKey(id)}.jpg`);
    if (dest.exists) return dest.uri; // 磁盘命中 → 零解码

    await acquire();
    try {
      const { uri } = await VideoThumbnails.getThumbnailAsync(localVideoUri, {
        time: 1000, // 毫秒;小偏移避开黑首帧(注意 expo-video 用秒,勿混)
        quality: 0.7,
      });
      // move 到确定性 keyed 路径(异步);overwrite 兜底极少见的并发竞态
      await new File(uri).move(dest, { overwrite: true });
      return dest.uri;
    } finally {
      release();
    }
  } catch {
    return null; // HEVC/HDR/异常容器等 → 调用方回退图标
  }
}
