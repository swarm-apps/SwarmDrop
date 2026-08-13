import { describe, expect, it, vi } from "vitest";
import { buildZip, planDirectoryZip, planItemZip } from "./zip-download";

const nested = [
  { relativePath: "photos/a.jpg", name: "a.jpg", size: 10 },
  { relativePath: "photos/2024/b.jpg", name: "b.jpg", size: 20 },
  { relativePath: "photos/2024/raw/c.dng", name: "c.dng", size: 30 },
];

describe("planItemZip", () => {
  /**
   * 这条是本次修复的**回归钉子**：逐个触发 `<a download>` 时被浏览器拦掉的恰好是
   * 深层那些文件（收件箱按 `file_id` 升序 = 发送侧 walkdir 顺序），线上症状是
   * 「目录只下载了第一层」。计划里少任何一层，那个 bug 就回来了。
   */
  it("keeps every level, not just the top one", () => {
    const plan = planItemZip(nested, "fallback");
    expect(plan.entries.map((entry) => entry.entryName)).toEqual([
      "photos/a.jpg",
      "photos/2024/b.jpg",
      "photos/2024/raw/c.dng",
    ]);
  });

  it("names the archive after the single top-level directory", () => {
    expect(planItemZip(nested, "fallback").fileName).toBe("photos.zip");
  });

  it("falls back when the files do not share one top-level directory", () => {
    const mixed = [
      { relativePath: "photos/a.jpg", name: "a.jpg", size: 1 },
      { relativePath: "notes/b.txt", name: "b.txt", size: 1 },
    ];
    expect(planItemZip(mixed, "收到的文件").fileName).toBe("收到的文件.zip");
  });

  it("falls back when a file sits at the root", () => {
    const flat = [{ relativePath: "a.jpg", name: "a.jpg", size: 1 }];
    expect(planItemZip(flat, "收到的文件").fileName).toBe("收到的文件.zip");
  });

  /** 体积是调用方判「要不要先警告一句」的依据——它必须只数进包的那些。 */
  it("reports the total size of what goes in", () => {
    expect(planItemZip(nested, "fallback").totalSize).toBe(60);
  });

  it("never produces a name that a download attribute would mangle", () => {
    const plan = planItemZip([], "a/b");
    expect(plan.fileName).toBe("a_b.zip");
    expect(planItemZip([], "  ").fileName).toBe("download.zip");
  });
});

describe("planDirectoryZip", () => {
  it("takes the whole subtree and keeps the folder itself as the zip root", () => {
    const plan = planDirectoryZip(nested, "photos/2024/");
    expect(plan.fileName).toBe("2024.zip");
    expect(plan.entries).toEqual([
      { sourcePath: "photos/2024/b.jpg", entryName: "2024/b.jpg" },
      { sourcePath: "photos/2024/raw/c.dng", entryName: "2024/raw/c.dng" },
    ]);
    // 只数子树里的那两个，不含 `photos/a.jpg`。
    expect(plan.totalSize).toBe(50);
  });

  it("keeps the top-level directory addressable too", () => {
    const plan = planDirectoryZip(nested, "photos/");
    expect(plan.fileName).toBe("photos.zip");
    expect(plan.entries.map((entry) => entry.entryName)).toEqual([
      "photos/a.jpg",
      "photos/2024/b.jpg",
      "photos/2024/raw/c.dng",
    ]);
  });

  /** 前缀相似的兄弟目录不能被卷进来——判据按 segment 边界比，不是裸 `startsWith`。 */
  it("does not swallow a sibling whose name shares the prefix", () => {
    const siblings = [
      { relativePath: "photos/2024/b.jpg", name: "b.jpg", size: 1 },
      { relativePath: "photos/2024-raw/c.dng", name: "c.dng", size: 1 },
    ];
    expect(
      planDirectoryZip(siblings, "photos/2024/").entries.map(
        (entry) => entry.sourcePath,
      ),
    ).toEqual(["photos/2024/b.jpg"]);
  });
});

describe("buildZip", () => {
  it("skips unreadable entries instead of failing the whole archive", async () => {
    const plan = planItemZip(nested, "fallback");
    const openFile = vi.fn(async (path: string) => {
      // OPFS 是配额存储，条目可能被浏览器驱逐——一个死路径不该让整包失败。
      if (path === "photos/2024/b.jpg") throw new Error("evicted");
      return new File([path], path.split("/").at(-1) ?? path);
    });

    const { blob, skipped } = await buildZip(plan, openFile, new Date(0));

    expect(skipped).toEqual(["photos/2024/b.jpg"]);
    expect(openFile).toHaveBeenCalledTimes(3);
    // 22 字节 = 空 zip 的中央目录结尾记录。装进了两个文件就必然比它大。
    expect(blob.size).toBeGreaterThan(22);
  });
});
