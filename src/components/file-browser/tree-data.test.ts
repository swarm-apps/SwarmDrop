import { describe, expect, it } from "vitest";
import { buildFileBrowserTree, normalizeRelativePath } from "./tree-data";
import type { FileBrowserItem } from "./types";

function item(id: string, relativePath: string, size = 10): FileBrowserItem {
  return { id, name: relativePath.split(/[\\/]/).pop()!, relativePath, size, status: "idle" };
}

describe("file browser tree data", () => {
  it("normalizes Windows and Unix separators", () => {
    expect(normalizeRelativePath("docs\\notes/readme.md")).toBe("docs/notes/readme.md");
    expect(normalizeRelativePath("./docs//readme.md")).toBe("docs/readme.md");
  });

  it("keeps same-name files in different directories distinct", () => {
    const tree = buildFileBrowserTree([
      item("a", "alpha/readme.md"),
      item("b", "beta/readme.md"),
    ]);
    expect(tree.nodes.get("file:a")?.relativePath).toBe("alpha/readme.md");
    expect(tree.nodes.get("file:b")?.relativePath).toBe("beta/readme.md");
  });

  it("aggregates nested directory size and count", () => {
    const tree = buildFileBrowserTree([
      item("a", "docs/a.txt", 4),
      item("b", "docs/deep/b.txt", 7),
      item("c", "root.txt", 2),
    ]);
    expect(tree.nodes.get("directory:docs/")).toMatchObject({ size: 11, fileCount: 2 });
    expect(tree.nodes.get("directory:docs/deep/")).toMatchObject({ size: 7, fileCount: 1 });
  });

  it("sorts directories before files and peers by natural name", () => {
    const tree = buildFileBrowserTree([
      item("z", "z.txt"),
      item("a", "folder-10/a.txt"),
      item("b", "folder-2/b.txt"),
      item("c", "a.txt"),
    ]);
    expect(tree.rootChildren).toEqual([
      "directory:folder-2/",
      "directory:folder-10/",
      "file:c",
      "file:z",
    ]);
  });
});
