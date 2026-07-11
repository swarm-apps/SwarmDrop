import type {
  FileBrowserItem,
  FileBrowserTreeData,
  FileBrowserTreeNode,
} from "./types";

export function normalizeRelativePath(path: string): string {
  return path
    .replace(/\\/g, "/")
    .split("/")
    .filter((part) => part.length > 0 && part !== "." && part !== "..")
    .join("/");
}

export function getParentPath(relativePath: string): string {
  const normalized = normalizeRelativePath(relativePath);
  const lastSeparator = normalized.lastIndexOf("/");
  return lastSeparator < 0 ? "" : normalized.slice(0, lastSeparator);
}

function directoryId(path: string): string {
  return `directory:${path}/`;
}

function fileNodeId(item: FileBrowserItem): string {
  return `file:${item.id}`;
}

function sortChildren(
  ids: string[],
  nodes: Map<string, FileBrowserTreeNode>,
): void {
  ids.sort((leftId, rightId) => {
    const left = nodes.get(leftId);
    const right = nodes.get(rightId);
    if (!left || !right) return leftId.localeCompare(rightId);
    if (left.type !== right.type) return left.type === "directory" ? -1 : 1;
    return left.name.localeCompare(right.name, undefined, {
      numeric: true,
      sensitivity: "base",
    });
  });
}

export function buildFileBrowserTree(
  items: FileBrowserItem[],
): FileBrowserTreeData {
  const nodes = new Map<string, FileBrowserTreeNode>();
  const children = new Map<string, string[]>();

  const addChild = (parentId: string, childId: string) => {
    const siblings = children.get(parentId) ?? [];
    if (!siblings.includes(childId)) siblings.push(childId);
    children.set(parentId, siblings);
  };

  for (const item of items) {
    const normalizedPath = normalizeRelativePath(item.relativePath || item.name);
    const parts = normalizedPath.split("/").filter(Boolean);
    const fileName = parts[parts.length - 1] || item.name;
    let parentId = "root";
    let currentPath = "";

    for (const part of parts.slice(0, -1)) {
      currentPath = currentPath ? `${currentPath}/${part}` : part;
      const id = directoryId(currentPath);
      if (!nodes.has(id)) {
        nodes.set(id, {
          id,
          name: part,
          type: "directory",
          relativePath: `${currentPath}/`,
          size: 0,
          fileCount: 0,
        });
        addChild(parentId, id);
      }
      const directory = nodes.get(id)!;
      directory.size += item.size;
      directory.fileCount = (directory.fileCount ?? 0) + 1;
      parentId = id;
    }

    const id = fileNodeId(item);
    nodes.set(id, {
      id,
      name: fileName,
      type: "file",
      relativePath: normalizedPath,
      size: item.size,
      item: { ...item, relativePath: normalizedPath },
    });
    addChild(parentId, id);
  }

  for (const ids of children.values()) sortChildren(ids, nodes);

  const root: FileBrowserTreeNode = {
    id: "root",
    name: "root",
    type: "directory",
    relativePath: "",
    size: items.reduce((sum, item) => sum + item.size, 0),
    fileCount: items.length,
  };
  nodes.set("root", root);

  return {
    nodes,
    children,
    rootChildren: children.get("root") ?? [],
    dataLoader: {
      getItem: (itemId) => nodes.get(itemId) ?? root,
      getChildren: (itemId) => children.get(itemId) ?? [],
    },
  };
}
