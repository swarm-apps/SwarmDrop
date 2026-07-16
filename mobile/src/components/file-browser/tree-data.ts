import { normalizeRelativePath } from "@/core/file-browser-identity";
import type { FileBrowserItem } from "./types";

export interface FileBrowserDirectoryRow {
  type: "directory";
  id: string;
  name: string;
  relativePath: string;
  depth: number;
  fileCount: number;
  size: bigint;
}

export interface FileBrowserFileRow {
  type: "file";
  id: string;
  item: FileBrowserItem;
  depth: number;
}

export type FileBrowserTreeRow = FileBrowserDirectoryRow | FileBrowserFileRow;

interface MutableDirectory {
  type: "directory";
  id: string;
  name: string;
  relativePath: string;
  depth: number;
  fileCount: number;
  size: bigint;
  children: MutableNode[];
}

interface MutableFile {
  type: "file";
  id: string;
  item: FileBrowserItem;
  depth: number;
}

type MutableNode = MutableDirectory | MutableFile;

export interface FileBrowserTree {
  roots: MutableNode[];
  directoryIds: ReadonlySet<string>;
}

export function buildFileBrowserTree(
  items: readonly FileBrowserItem[],
): FileBrowserTree {
  const root: MutableDirectory = {
    type: "directory",
    id: "dir:",
    name: "",
    relativePath: "",
    depth: -1,
    fileCount: 0,
    size: 0n,
    children: [],
  };
  const directories = new Map<string, MutableDirectory>([["", root]]);

  for (const item of items) {
    const relativePath = normalizeRelativePath(item.relativePath, item.name);
    const segments = relativePath.split("/");
    let parent = root;
    let currentPath = "";

    for (const segment of segments.slice(0, -1)) {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;
      let directory = directories.get(currentPath);
      if (!directory) {
        directory = {
          type: "directory",
          id: `dir:${currentPath}/`,
          name: segment,
          relativePath: `${currentPath}/`,
          depth: currentPath.split("/").length - 1,
          fileCount: 0,
          size: 0n,
          children: [],
        };
        directories.set(currentPath, directory);
        parent.children.push(directory);
      }
      parent = directory;
    }

    parent.children.push({
      type: "file",
      id: `file:${item.id}`,
      item: { ...item, relativePath },
      depth: segments.length - 1,
    });

    root.fileCount += 1;
    root.size += item.size;
    let ancestorPath = "";
    for (const segment of segments.slice(0, -1)) {
      ancestorPath = ancestorPath ? `${ancestorPath}/${segment}` : segment;
      const directory = directories.get(ancestorPath);
      if (directory) {
        directory.fileCount += 1;
        directory.size += item.size;
      }
    }
  }

  for (const directory of directories.values()) {
    directory.children.sort(compareTreeNodes);
  }

  return {
    roots: root.children,
    directoryIds: new Set(
      [...directories.values()]
        .filter((directory) => directory !== root)
        .map((directory) => directory.id),
    ),
  };
}

export function flattenVisibleNodes(
  tree: FileBrowserTree,
  expandedIds: ReadonlySet<string>,
): FileBrowserTreeRow[] {
  const rows: FileBrowserTreeRow[] = [];
  const visit = (nodes: readonly MutableNode[]) => {
    for (const node of nodes) {
      if (node.type === "file") {
        rows.push(node);
        continue;
      }
      const { children: _children, ...row } = node;
      rows.push(row);
      if (expandedIds.has(node.id)) visit(node.children);
    }
  };
  visit(tree.roots);
  return rows;
}

function compareTreeNodes(a: MutableNode, b: MutableNode): number {
  if (a.type !== b.type) return a.type === "directory" ? -1 : 1;
  const aName = a.type === "directory" ? a.name : a.item.name;
  const bName = b.type === "directory" ? b.name : b.item.name;
  return aName.localeCompare(bName) || a.id.localeCompare(b.id);
}
