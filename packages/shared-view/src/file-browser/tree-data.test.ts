import { describe, expect, it } from "vitest";
import { buildFileBrowserTree, flattenVisibleNodes } from "./tree-data";
import type { FileBrowserItem } from "./types";

function file(relativePath: string, size = 100): FileBrowserItem {
  const name = relativePath.split("/").pop() ?? relativePath;
  return { id: relativePath, name, relativePath, size, status: "idle" };
}

describe("buildFileBrowserTree", () => {
  it("从相对路径派生目录层级", () => {
    const tree = buildFileBrowserTree([file("a/b/c.txt"), file("a/d.txt"), file("e.txt")]);

    // 顶层：目录 a 与文件 e.txt
    expect(tree.roots.map((n) => (n.type === "directory" ? n.name : n.item.name))).toEqual([
      "a",
      "e.txt",
    ]);

    const a = tree.roots[0];
    expect(a.type).toBe("directory");
    if (a.type !== "directory") throw new Error("unreachable");
    expect(a.relativePath).toBe("a/"); // 带尾斜杠，供前缀匹配
    expect(a.children.map((n) => (n.type === "directory" ? n.name : n.item.name))).toEqual([
      "b",
      "d.txt",
    ]);
  });

  it("目录的 size 与 fileCount 是递归累计，含所有后代", () => {
    const tree = buildFileBrowserTree([file("a/b/c.txt", 10), file("a/d.txt", 20)]);
    const a = tree.roots[0];
    if (a.type !== "directory") throw new Error("unreachable");

    expect(a.fileCount).toBe(2);
    expect(a.size).toBe(30);

    const b = a.children.find((n) => n.type === "directory");
    if (b?.type !== "directory") throw new Error("unreachable");
    expect(b.fileCount).toBe(1);
    expect(b.size).toBe(10);

    expect(tree.totalCount).toBe(2);
    expect(tree.totalSize).toBe(30);
  });

  it("目录排在文件前，其次按名称自然序（file2 在 file10 之前）", () => {
    const tree = buildFileBrowserTree([
      file("file10.txt"),
      file("file2.txt"),
      file("zdir/x.txt"),
    ]);
    expect(tree.roots.map((n) => (n.type === "directory" ? n.name : n.item.name))).toEqual([
      "zdir",
      "file2.txt",
      "file10.txt",
    ]);
  });

  it("归一后的路径回写进节点的 item", () => {
    // 反斜杠 + 冗余段 + 路径穿越
    const tree = buildFileBrowserTree([file("a\\..\\b//c.txt")]);
    const a = tree.roots[0];
    if (a.type !== "directory") throw new Error("unreachable");
    expect(a.name).toBe("a");
    const b = a.children[0];
    if (b?.type !== "directory") throw new Error("unreachable");
    const c = b.children[0];
    if (c?.type !== "file") throw new Error("unreachable");
    expect(c.item.relativePath).toBe("a/b/c.txt");
  });

  it("同一目录下的多个文件复用同一个目录节点", () => {
    const tree = buildFileBrowserTree([file("a/1.txt"), file("a/2.txt"), file("a/3.txt")]);
    expect(tree.roots).toHaveLength(1);
    expect(tree.directoryIds.size).toBe(1);
  });

  it("depth 从 0 起算（顶层文件为 0）", () => {
    const tree = buildFileBrowserTree([file("top.txt"), file("a/b/deep.txt")]);
    const top = tree.roots.find((n) => n.type === "file");
    expect(top?.depth).toBe(0);
    const a = tree.roots.find((n) => n.type === "directory");
    expect(a?.depth).toBe(0);
  });

  it("空输入给出空树", () => {
    const tree = buildFileBrowserTree([]);
    expect(tree.roots).toEqual([]);
    expect(tree.totalCount).toBe(0);
    expect(tree.totalSize).toBe(0);
  });
});

describe("flattenVisibleNodes", () => {
  it("只展开列在 expandedIds 里的目录", () => {
    const tree = buildFileBrowserTree([file("a/b/c.txt"), file("z.txt")]);
    const a = tree.roots[0];
    if (a.type !== "directory") throw new Error("unreachable");

    const collapsed = flattenVisibleNodes(tree, new Set());
    expect(collapsed.map((n) => n.id)).toEqual([a.id, "file:z.txt"]);

    const expanded = flattenVisibleNodes(tree, new Set([a.id]));
    // a → b（b 未展开，其子不出现）→ z.txt
    expect(expanded).toHaveLength(3);
    expect(expanded[1].type).toBe("directory");
  });

  it("全展开时每个节点各出现一次", () => {
    const tree = buildFileBrowserTree([file("a/b/c.txt")]);
    const rows = flattenVisibleNodes(tree, tree.directoryIds);
    expect(rows.map((n) => n.type)).toEqual(["directory", "directory", "file"]);
  });

  // 行携带整棵子树会让虚拟列表的 diff 变成深比较；层级已由 depth 表达。
  it("目录行不带 children", () => {
    const tree = buildFileBrowserTree([file("a/b.txt")]);
    const rows = flattenVisibleNodes(tree, tree.directoryIds);
    const directory = rows.find((n) => n.type === "directory");
    expect(directory).toBeDefined();
    expect("children" in (directory as object)).toBe(false);
  });
});
