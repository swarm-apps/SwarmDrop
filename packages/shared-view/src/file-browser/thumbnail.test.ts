import { describe, expect, it } from "vitest";
import {
  THUMBNAIL_MAX_EDGE,
  THUMBNAIL_MAX_SOURCE_BYTES,
  shouldGenerateThumbnail,
  thumbnailCacheKey,
  thumbnailTargetSize,
} from "./thumbnail";
import type { FileBrowserItem } from "./types";

function item(overrides: Partial<FileBrowserItem> = {}): FileBrowserItem {
  return {
    id: "x",
    name: "photo.jpg",
    relativePath: "photo.jpg",
    size: 1024,
    status: "completed",
    previewSource: "opfs:/photo.jpg",
    ...overrides,
  };
}

describe("shouldGenerateThumbnail", () => {
  it("图片 + 有取图源 + 大小在门槛内 → 生成", () => {
    expect(shouldGenerateThumbnail(item())).toBe(true);
  });

  it("非图片类型不生成", () => {
    expect(shouldGenerateThumbnail(item({ name: "archive.zip" }))).toBe(false);
  });

  it("没有取图源不生成", () => {
    expect(shouldGenerateThumbnail(item({ previewSource: undefined }))).toBe(false);
  });

  // 缩略图省的是缓存驻留，省不掉解码峰值——超大图解码那一刻就能把标签页顶穿。
  it("超过尺寸门槛不生成", () => {
    expect(shouldGenerateThumbnail(item({ size: THUMBNAIL_MAX_SOURCE_BYTES + 1 }))).toBe(false);
    expect(shouldGenerateThumbnail(item({ size: THUMBNAIL_MAX_SOURCE_BYTES }))).toBe(true);
  });

  it("缺失的文件不生成——它已经不在盘上，取图只会白白失败一次", () => {
    expect(shouldGenerateThumbnail(item({ status: "missing" }))).toBe(false);
  });
});

describe("thumbnailCacheKey", () => {
  // 同一份字节在不同场景有不同展示 ID（source: / inbox:），缩略图可以共用。
  it("按取图源而非展示 ID 构造，跨场景可复用", () => {
    const a = thumbnailCacheKey(item({ id: "source:/a.jpg" }));
    const b = thumbnailCacheKey(item({ id: "inbox:1:file:2" }));
    expect(a).toBe(b);
  });

  it("大小变了 key 就变，让旧缩略图自然失效", () => {
    expect(thumbnailCacheKey(item({ size: 1 }))).not.toBe(thumbnailCacheKey(item({ size: 2 })));
  });

  it("没有取图源时返回 null", () => {
    expect(thumbnailCacheKey(item({ previewSource: undefined }))).toBeNull();
  });
});

describe("thumbnailTargetSize", () => {
  it("按长边缩放，保持比例", () => {
    expect(thumbnailTargetSize(1600, 1200)).toEqual({ width: 320, height: 240 });
    expect(thumbnailTargetSize(1200, 1600)).toEqual({ width: 240, height: 320 });
  });

  it("不放大小图", () => {
    expect(thumbnailTargetSize(100, 80)).toEqual({ width: 100, height: 80 });
  });

  it("长边恰好等于上限时原样返回", () => {
    expect(thumbnailTargetSize(THUMBNAIL_MAX_EDGE, 100)).toEqual({
      width: THUMBNAIL_MAX_EDGE,
      height: 100,
    });
  });

  // 小数宽高会让不同端因取整方向不同而产出差一像素的图，缓存就不能跨端复用了。
  it("返回整数像素，且极端比例下不塌成 0", () => {
    const size = thumbnailTargetSize(10_000, 3);
    expect(Number.isInteger(size.width)).toBe(true);
    expect(Number.isInteger(size.height)).toBe(true);
    expect(size.height).toBeGreaterThanOrEqual(1);
  });

  it("非法输入给出 0×0 而不是 NaN", () => {
    expect(thumbnailTargetSize(0, 0)).toEqual({ width: 0, height: 0 });
  });
});
