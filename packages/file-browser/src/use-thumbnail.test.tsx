/**
 * `useThumbnail` 的三条护栏：去重、并发上限、淘汰即 revoke。
 *
 * 这三件事都**没有反馈回路**——漏了不会报错，只会在用户滚一个几百文件的收件箱时把标签页
 * 顶崩，或者悄悄泄漏一串 object URL（每个都持有一张解码后的位图）。所以它们靠测试钉住。
 *
 * jsdom 没有 `createImageBitmap` / `OffscreenCanvas` / `IntersectionObserver`，全部打桩；
 * 桩的语义与真实 API 一致（bitmap 要 `close`、canvas 产出 Blob）。
 */

import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileBrowserItem } from "@swarmdrop/shared-view";
import {
  THUMBNAIL_CONCURRENCY,
  __resetThumbnailCache,
  useThumbnail,
} from "./use-thumbnail";

let createdUrls: string[];
let revokedUrls: string[];
let urlSeq: number;
/** 当前正在解码的数量与历史峰值——并发上限就是靠峰值断言的。 */
let decoding: number;
let peakDecoding: number;
/** 每次解码的 gate：测试主动放行，才能观察到「排队」这个状态。 */
let releases: Array<() => void>;

function makeItem(id: string, previewSource: string): FileBrowserItem {
  return {
    id,
    name: `${id}.png`,
    relativePath: `${id}.png`,
    size: 1024,
    status: "completed",
    previewSource,
  };
}

function Probe({ item, resolver }: { item: FileBrowserItem; resolver: () => Promise<Blob | null> }) {
  const { ref, url } = useThumbnail(item, resolver);
  return <div ref={ref} data-testid={item.id} data-url={url ?? ""} />;
}

beforeEach(() => {
  createdUrls = [];
  revokedUrls = [];
  urlSeq = 0;
  decoding = 0;
  peakDecoding = 0;
  releases = [];

  vi.stubGlobal("IntersectionObserver", class {
    constructor(private readonly callback: IntersectionObserverCallback) {}
    observe() {
      // 立即当作进入视口——视口触发本身由浏览器负责，这里要测的是它之后的那一段。
      this.callback(
        [{ isIntersecting: true } as IntersectionObserverEntry],
        this as unknown as IntersectionObserver,
      );
    }
    disconnect() {}
    unobserve() {}
    takeRecords() {
      return [];
    }
  });

  vi.stubGlobal("createImageBitmap", async () => {
    decoding += 1;
    peakDecoding = Math.max(peakDecoding, decoding);
    await new Promise<void>((resolve) => releases.push(resolve));
    return { width: 800, height: 600, close: () => { decoding -= 1; } } as unknown as ImageBitmap;
  });

  vi.stubGlobal("OffscreenCanvas", class {
    constructor(public width: number, public height: number) {}
    getContext() {
      return { drawImage: () => {} };
    }
    async convertToBlob() {
      return new Blob();
    }
  });

  vi.stubGlobal("URL", {
    createObjectURL: () => {
      const url = `blob:thumb-${(urlSeq += 1)}`;
      createdUrls.push(url);
      return url;
    },
    revokeObjectURL: (url: string) => revokedUrls.push(url),
  });
});

afterEach(() => {
  cleanup();
  // 缓存是模块级的：不清的话，上一个用例塞进去的条目会算进下一个用例的淘汰数。
  __resetThumbnailCache();
  revokedUrls.length = 0;
  vi.unstubAllGlobals();
});

/**
 * 放行所有排队中的解码，直到连着几轮都没有新的排上来。
 *
 * 「连着几轮」而不是「一轮为空就停」：一次放行到下一批 `createImageBitmap` 排上来，中间隔着
 * 好几个 await（槽位交接 → resolver → 桩本身），单个宏任务不一定够。
 */
async function drain() {
  let idle = 0;
  for (let round = 0; round < 200 && idle < 4; round += 1) {
    if (releases.length === 0) {
      idle += 1;
    } else {
      idle = 0;
      const pending = releases;
      releases = [];
      pending.forEach((release) => release());
    }
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

describe("useThumbnail", () => {
  it("同一份字节只解码一次，两个卡片共用一张缩略图", async () => {
    const resolver = vi.fn(async () => new Blob());
    // 展示 ID 不同（发送侧 / 收件箱是两个 scope），但取图源与大小相同——
    // L1 的 `thumbnailCacheKey` 定的就是这条：同一份字节共用缓存。
    render(
      <>
        <Probe item={makeItem("a", "photo.png")} resolver={resolver} />
        <Probe item={makeItem("b", "photo.png")} resolver={resolver} />
      </>,
    );

    await waitFor(() => expect(releases.length).toBeGreaterThan(0));
    await drain();

    expect(resolver).toHaveBeenCalledTimes(1);
    expect(createdUrls).toHaveLength(1);
    await waitFor(() => {
      const a = document.querySelector('[data-testid="a"]')?.getAttribute("data-url");
      const b = document.querySelector('[data-testid="b"]')?.getAttribute("data-url");
      expect(a).toBe(createdUrls[0]);
      expect(b).toBe(createdUrls[0]);
    });
  });

  it("并发解码不超过 THUMBNAIL_CONCURRENCY", async () => {
    const resolver = vi.fn(async () => new Blob());
    const count = THUMBNAIL_CONCURRENCY * 3;
    render(
      <>
        {Array.from({ length: count }, (_, index) => (
          <Probe key={index} item={makeItem(`n${index}`, `n${index}.png`)} resolver={resolver} />
        ))}
      </>,
    );

    await waitFor(() => expect(releases.length).toBeGreaterThan(0));
    await drain();

    expect(resolver).toHaveBeenCalledTimes(count);
    expect(peakDecoding).toBeLessThanOrEqual(THUMBNAIL_CONCURRENCY);
    expect(peakDecoding).toBeGreaterThan(0);
  });

  // 上一条只覆盖了「全部同时到达」。真正难的是**错峰到达**：先满员，放行几个之后新的再进来。
  // 槽位若是「先减计数、再唤醒等待者」，两者之间隔着一个微任务——恰好落在那段窗口里的新请求
  // 会看到「还有空位」而直接放行，等被唤醒者再自增，同时在跑的就超了。滚动网格就是这个形状。
  it("错峰到达时并发上限依然成立", async () => {
    const resolver = vi.fn(async () => new Blob());
    const first = THUMBNAIL_CONCURRENCY * 2;
    const { rerender } = render(
      <>
        {Array.from({ length: first }, (_, index) => (
          <Probe key={index} item={makeItem(`s${index}`, `s${index}.png`)} resolver={resolver} />
        ))}
      </>,
    );
    await waitFor(() => expect(releases.length).toBeGreaterThan(0));

    // 放行一批（腾出槽位），紧接着在同一拍里挂上新的一批。
    const firstBatch = releases;
    releases = [];
    firstBatch.forEach((release) => release());
    rerender(
      <>
        {Array.from({ length: first + THUMBNAIL_CONCURRENCY * 2 }, (_, index) => (
          <Probe key={index} item={makeItem(`s${index}`, `s${index}.png`)} resolver={resolver} />
        ))}
      </>,
    );
    await drain();

    expect(peakDecoding).toBeLessThanOrEqual(THUMBNAIL_CONCURRENCY);
  });

  it("超出 LRU 容量时淘汰最旧的一条并 revoke 它的 URL", async () => {
    const resolver = vi.fn(async () => new Blob());
    // 容量是 64（模块私有），一次性渲染 70 个把它顶穿。
    const count = 70;
    render(
      <>
        {Array.from({ length: count }, (_, index) => (
          <Probe key={index} item={makeItem(`e${index}`, `e${index}.png`)} resolver={resolver} />
        ))}
      </>,
    );

    await waitFor(() => expect(releases.length).toBeGreaterThan(0));
    await drain();

    await waitFor(() => expect(createdUrls).toHaveLength(count));
    // 溢出多少就该 revoke 多少——一条不 revoke 就是一张解码后的位图留在内存里。
    expect(revokedUrls.length).toBe(count - 64);
    expect(revokedUrls[0]).toBe(createdUrls[0]);
  });

  it("不给 resolver 就完全不取图（桌面那条路径）", async () => {
    const resolver = vi.fn(async () => new Blob());
    render(<Probe item={makeItem("desktop", "asset://photo.png")} resolver={undefined as never} />);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(resolver).not.toHaveBeenCalled();
    expect(createdUrls).toHaveLength(0);
  });
});
