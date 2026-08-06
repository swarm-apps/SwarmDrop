/**
 * 缩略图管线 —— 取图源由调用方注入，缩放 / 缓存 / 并发 / 视口触发在这里。
 *
 * 分工与 L1 的 `thumbnail.ts` 是这样切的：**判定与规格在 L1**（该不该生成、缩到多大、
 * 缓存 key 怎么算——三端必须一致），**管线在这里**（`createImageBitmap` / canvas 是 DOM API，
 * 进不了 L1 那道无 DOM 的门禁）。
 *
 * ```
 * previewSource →（调用方的 resolver）→ Blob
 *   → createImageBitmap → canvas(≤320px 长边)
 *   → toBlob({ type: "image/webp", quality: 0.7 })
 *   → createObjectURL → LRU(64)
 * ```
 *
 * **桌面不走这条路**：它的 `previewSource` 已经是 `convertFileSrc` 给的 asset URL，直接能当
 * `<img src>`。真正需要管线的是 Web——OPFS 里的文件只能异步取成 `File`，没有可直接渲染的 URL。
 * 所以 resolver 不传时，`FileCard` 回落到 `item.previewSource` 本身。
 */

import { useEffect, useRef, useState } from "react";
import {
  shouldGenerateThumbnail,
  thumbnailCacheKey,
  thumbnailTargetSize,
} from "@swarmdrop/shared-view";
import type { FileBrowserItem } from "@swarmdrop/shared-view";
import type { ThumbnailResolver } from "./types";

/**
 * 同时在解码的数量上限。
 *
 * 解码是 CPU 与内存双重密集的操作，而 L1 的尺寸门槛只挡住了单张的极端值——十张 5 MB
 * 的图同时解码一样能把标签页顶穿。串行太慢（滚动时肉眼可见地一张张冒出来），4 是折中。
 *
 * **住在 L2 而不是 L1**：它是本文件这个 JS 解码闸门的旋钮，不描述任何跨端行为
 * （移动端是原生解码，调度归原生侧）。导出只为测试断言，不进 `index.ts` 的公开面。
 */
export const THUMBNAIL_CONCURRENCY = 4;

/**
 * LRU 容量。
 *
 * 一条缩略图约 30 KB，64 条 ≈ 2 MB——比一屏网格（十几张）宽裕得多，滚上去再滚回来不必重解码，
 * 又不至于在一个几百文件的收件箱条目里无限增长。
 */
const CACHE_LIMIT = 64;

/**
 * 视口预取边距。
 *
 * 卡片进入视口前 200px 就开始解码，滚动时用户看到的是已经画好的图而不是先图标后突变。
 * 再大就等于放弃了「只对进入视口的项触发」这条约束本身。
 */
const PREFETCH_MARGIN = "200px";

/**
 * 模块级共享缓存：`thumbnailCacheKey(item)` → object URL。
 *
 * **key 用 `previewSource#size` 而不是 `item.id`**（L1 的 `thumbnailCacheKey` 定的），
 * 所以同一份字节在不同场景（发送侧 / 收件箱）出现时共用同一张缩略图。
 *
 * `Map` 的插入序即 LRU 序：命中时 delete + set 把它挪到末尾，超限时淘汰第一个。
 */
const cache = new Map<string, string>();

/** 同一个 key 正在生成时的去重表——一屏里同一份字节出现两次不该解码两遍。 */
const inflight = new Map<string, Promise<string | null>>();

/**
 * 取不到 / 解不开的 key。**没有它就是一条重试风暴**：虚拟列表会卸载视口外的行，滚回来
 * 重新挂载 → effect 重跑 → 正缓存里没有 → 又走一遍完整链路（最坏是 OPFS 的 5s 超时），
 * 还每次占一个解码槽。收件箱里一批未落盘完成的条目，上下滚一次就能把管线打满。
 *
 * 与正缓存共用一个上限，超了就整个清空——它只装字符串，清掉的代价仅仅是下次重试一遍。
 */
const failed = new Set<string>();

function cacheGet(key: string): string | undefined {
  const url = cache.get(key);
  if (url === undefined) return undefined;
  cache.delete(key);
  cache.set(key, url);
  return url;
}

function cachePut(key: string, url: string): void {
  cache.set(key, url);
  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next();
    if (oldest.done) break;
    const evicted = cache.get(oldest.value);
    cache.delete(oldest.value);
    // 淘汰即 revoke——object URL 不 revoke 就是泄漏，它持有的是整张解码后的位图。
    if (evicted) URL.revokeObjectURL(evicted);
  }
}

/* ─── 并发闸门 ─── */

let activeSlots = 0;
const waiting: Array<() => void> = [];

/**
 * 拿一个解码槽，用完归还。
 *
 * 解码是 CPU 与内存双重密集的操作，而尺寸门槛（L1 的 `THUMBNAIL_MAX_SOURCE_BYTES`）
 * 只挡住了单张的极端值——十张 5 MB 的图同时解码一样能把标签页顶崩。
 *
 * **槽位是直接转交给下一个等待者的，不经过 `activeSlots`。** 这不是写法偏好：先减计数
 * 再唤醒的话，两者之间隔着一个微任务，恰好在这段窗口里进来的新请求会看到「还有空位」
 * 而直接放行，等被唤醒者再自增，同时在跑的就超出了上限。滚动网格时新请求正是一个接一个
 * 到达的，这条窗口一点都不罕见。
 */
async function withSlot<T>(fn: () => Promise<T>): Promise<T> {
  if (activeSlots >= THUMBNAIL_CONCURRENCY) {
    // 被唤醒即已持有槽位（见下面的转交），所以这条分支不自增。
    await new Promise<void>((resolve) => waiting.push(resolve));
  } else {
    activeSlots += 1;
  }
  try {
    return await fn();
  } finally {
    const next = waiting.shift();
    if (next) next();
    else activeSlots -= 1;
  }
}

/**
 * 清空缓存并 revoke 全部 URL。**仅供测试**——缓存是模块级的，用例之间会互相污染
 * （上一个用例塞进去的条目会算进下一个用例的淘汰数）。
 *
 * 刻意不从 `index.ts` 导出：它不是本包对外契约的一部分，测试直接 import 本模块。
 */
export function __resetThumbnailCache(): void {
  for (const url of cache.values()) URL.revokeObjectURL(url);
  cache.clear();
  inflight.clear();
  // 并发闸门也要清：用例结束时可能还有没被放行的等待者，槽位计数会带着走进下一个用例，
  // 表现是「下一个用例里一张图都不解码」——排查起来完全不像是上一个用例留下的。
  failed.clear();
  const stranded = waiting.splice(0, waiting.length);
  stranded.forEach((release) => release());
  activeSlots = 0;
}

/* ─── 缩放 ─── */

/**
 * `Blob` → 缩略图 object URL。失败一律返回 `null`（调用方降级到类型图标）。
 *
 * `OffscreenCanvas` 不是哪都有（Safari 16 之前没有 `convertToBlob`），所以带一条
 * `<canvas>` 回退。两条路的输出规格一致——尺寸由 L1 的 `thumbnailTargetSize` 定，
 * 三端同一个取整规则。
 */
async function renderThumbnail(source: Blob): Promise<string | null> {
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(source);
  } catch {
    // 损坏的图 / 浏览器不认的编码。不是异常路径，是「这个文件没有预览」。
    return null;
  }
  try {
    const { width, height } = thumbnailTargetSize(bitmap.width, bitmap.height);
    if (width === 0 || height === 0) return null;
    const blob = await drawToBlob(bitmap, width, height);
    return blob ? URL.createObjectURL(blob) : null;
  } catch {
    return null;
  } finally {
    // 位图不 close 就要等 GC，而它持有的是完整解码后的像素。
    bitmap.close();
  }
}

async function drawToBlob(
  bitmap: ImageBitmap,
  width: number,
  height: number,
): Promise<Blob | null> {
  if (typeof OffscreenCanvas !== "undefined") {
    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.drawImage(bitmap, 0, 0, width, height);
    return canvas.convertToBlob({ type: "image/webp", quality: 0.7 });
  }
  if (typeof document === "undefined") return null;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(bitmap, 0, 0, width, height);
  return new Promise<Blob | null>((resolve) => {
    canvas.toBlob((blob) => resolve(blob), "image/webp", 0.7);
  });
}

async function loadThumbnail(
  key: string,
  previewSource: string,
  resolve: ThumbnailResolver,
): Promise<string | null> {
  if (failed.has(key)) return null;
  const pending = inflight.get(key);
  if (pending) return pending;

  // **取字节在闸门之外**。闸门存在的理由是「解码是 CPU 与内存双重密集」，而取字节是纯
  // I/O（Web 那条是 OPFS 遍历，最坏 5s 超时）。把它圈进来的话，一个取不到的文件就冻结
  // 四分之一的管线最长 5 秒，而它一个字节都没在解码。
  const task = (async () => {
    const source = await resolve(previewSource).catch(() => null);
    if (!source) return null;
    return withSlot(() => renderThumbnail(source));
  })()
    .then((url) => {
      if (url) cachePut(key, url);
      else rememberFailure(key);
      return url;
    })
    .finally(() => inflight.delete(key));

  inflight.set(key, task);
  return task;
}

function rememberFailure(key: string): void {
  if (failed.size >= CACHE_LIMIT) failed.clear();
  failed.add(key);
}

/* ─── Hook ─── */

/**
 * 解析一个条目的网格缩略图。
 *
 * 返回的 `ref` 要挂到卡片的预览容器上——解码**只对进入视口的条目触发**（一个几百文件的
 * 收件箱条目一次性全解码会直接卡死）。
 *
 * `resolver` **必须是稳定引用**（调用方 `useCallback` 或模块级函数）。它进 effect 依赖，
 * 每次渲换一个新的会让每一帧都重跑一遍取图。
 */
export function useThumbnail(
  item: FileBrowserItem,
  resolver?: ThumbnailResolver,
): { ref: React.RefObject<HTMLDivElement | null>; url: string | undefined } {
  const ref = useRef<HTMLDivElement | null>(null);
  const previewSource = item.previewSource;
  const key =
    resolver && previewSource && shouldGenerateThumbnail(item)
      ? thumbnailCacheKey(item)
      : null;

  // 状态与 key 绑在一起。网格是虚拟滚动的，同一个组件实例会被复用给不同条目——只存 url
  // 的话，复用那一帧会先闪出上一个文件的缩略图（移动端 `useFileThumbnail` 记的是同一条）。
  // 用 React 官方的「随 prop 变化在渲染期重置 state」模式，命中缓存则同步取回。
  const [state, setState] = useState<{ key: string | null; url: string | undefined }>(() => ({
    key,
    url: key ? cacheGet(key) : undefined,
  }));
  if (state.key !== key) {
    setState({ key, url: key ? cacheGet(key) : undefined });
  }

  // **视口观察与取图是同一条 effect。** 拆成「effect A 置 visible → 重渲染 → effect B 取图」
  // 会让每张卡片多一次纯粹为了传递布尔量的渲染，而网格正是成批挂载的地方。
  useEffect(() => {
    const element = ref.current;
    if (!key || !previewSource || !resolver || !element) return;
    let cancelled = false;

    const start = () => {
      const cached = cacheGet(key);
      if (cached) {
        // 内容没变就返回 `prev` 本身：新对象 `Object.is` 判不等，会白广播一轮。
        setState((prev) =>
          prev.key === key && prev.url === cached ? prev : { key, url: cached },
        );
        return;
      }
      void loadThumbnail(key, previewSource, resolver).then((url) => {
        if (cancelled || !url) return;
        // 只在组件当前仍绑着同一个 key 时写入——解码回来时它可能已经被复用给别的条目了。
        setState((prev) => (prev.key === key ? { key, url } : prev));
      });
    };

    // jsdom 与老浏览器没有 IntersectionObserver：直接当作可见，宁可多解码也不要不出图。
    if (typeof IntersectionObserver === "undefined") {
      start();
      return () => {
        cancelled = true;
      };
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        // 进过一次视口就不必再观察了——取图有缓存兜底，反复触发只是白跑。
        observer.disconnect();
        start();
      },
      { rootMargin: PREFETCH_MARGIN },
    );
    observer.observe(element);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [key, previewSource, resolver]);

  // 卸载**不** revoke：URL 在共享缓存里，别的卡片可能正用着同一张。回收只发生在 LRU 淘汰。
  //
  // 返回 `state.url` 不必再守 `state.key === key`：上面那次渲染期 `setState` 会让 React
  // 立刻重跑本函数并丢弃这一遍的输出，真正提交出去的那一遍里两者必然相等。
  return { ref, url: state.url };
}
