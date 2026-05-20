/**
 * 文件树数据构建 —— 纯逻辑，与桌面端 `crates SwarmDrop/src/components/file-tree/data.ts` 同源。
 *
 * 输入：flat 文件数组（每个含 relativePath / name / size / fileId?）
 * 输出：headless-tree 可消费的 `TreeDataLoader` + 根级 children id 列表
 *
 * 不依赖 DOM / web API，可直接在 RN 跑。
 */

export type FileStatus =
  | "select"
  | "waiting"
  | "transferring"
  | "completed"
  | "error";

export interface TreeNodeData {
  id: string;
  name: string;
  type: "file" | "directory";
  path: string;
  size: number;
  fileId?: number;
  fileCount?: number;
  absolutePath?: string;
}

export interface TreeDataLoader {
  getItem: (itemId: string) => TreeNodeData;
  getChildren: (itemId: string) => string[];
}

export interface TreeData {
  dataLoader: TreeDataLoader;
  rootChildren: string[];
}

interface FileEntry {
  fileId?: number;
  name: string;
  relativePath: string;
  size: number;
  absolutePath?: string;
}

function sortChildren(
  children: string[],
  nodeMap: Map<string, TreeNodeData>,
): void {
  children.sort((a, b) => {
    const aIsDir = a.endsWith("/");
    const bIsDir = b.endsWith("/");
    if (aIsDir !== bIsDir) return aIsDir ? -1 : 1;
    return (nodeMap.get(a)?.name ?? "").localeCompare(
      nodeMap.get(b)?.name ?? "",
    );
  });
}

function computeFileCount(
  nodeId: string,
  nodeMap: Map<string, TreeNodeData>,
  childrenMap: Map<string, string[]>,
): number {
  const children = childrenMap.get(nodeId) ?? [];
  let count = 0;
  for (const childId of children) {
    const child = nodeMap.get(childId);
    if (child?.type === "directory") {
      const childCount = computeFileCount(childId, nodeMap, childrenMap);
      child.fileCount = childCount;
      count += childCount;
    } else {
      count++;
    }
  }
  return count;
}

function buildTreeFromEntries(entries: FileEntry[]): TreeData {
  const nodeMap = new Map<string, TreeNodeData>();
  const childrenMap = new Map<string, string[]>();

  const sorted = [...entries].sort((a, b) =>
    a.relativePath.localeCompare(b.relativePath),
  );

  for (const entry of sorted) {
    const { relativePath, name, size, fileId, absolutePath } = entry;
    const fileNodeId = relativePath;

    nodeMap.set(fileNodeId, {
      id: fileNodeId,
      name,
      type: "file",
      path: relativePath,
      size,
      ...(fileId !== undefined && { fileId }),
      ...(absolutePath !== undefined && { absolutePath }),
    });

    const parts = relativePath.split("/");
    let currentPath = "";

    for (let i = 0; i < parts.length - 1; i++) {
      const parentPath = currentPath;
      currentPath = currentPath ? `${currentPath}/${parts[i]}` : parts[i];
      const dirId = `${currentPath}/`;

      if (!nodeMap.has(dirId)) {
        nodeMap.set(dirId, {
          id: dirId,
          name: parts[i],
          type: "directory",
          path: dirId,
          size: 0,
        });

        const parentId = parentPath ? `${parentPath}/` : "root";
        const siblings = childrenMap.get(parentId) ?? [];
        siblings.push(dirId);
        childrenMap.set(parentId, siblings);
      }

      const dirNode = nodeMap.get(dirId);
      if (dirNode) dirNode.size += size;
    }

    const parentDir =
      parts.length > 1 ? `${parts.slice(0, -1).join("/")}/` : "root";
    const siblings = childrenMap.get(parentDir) ?? [];
    siblings.push(fileNodeId);
    childrenMap.set(parentDir, siblings);
  }

  for (const children of childrenMap.values()) {
    sortChildren(children, nodeMap);
  }

  const rootChildren = childrenMap.get("root") ?? [];
  for (const rootId of rootChildren) {
    const node = nodeMap.get(rootId);
    if (node?.type === "directory") {
      node.fileCount = computeFileCount(rootId, nodeMap, childrenMap);
    }
  }

  const totalSize = sorted.reduce((sum, e) => sum + e.size, 0);
  nodeMap.set("root", {
    id: "root",
    name: "root",
    type: "directory",
    path: "",
    size: totalSize,
  });

  const dataLoader: TreeDataLoader = {
    getItem: (itemId) =>
      nodeMap.get(itemId) ?? {
        id: itemId,
        name: itemId,
        type: "file",
        path: itemId,
        size: 0,
      },
    getChildren: (itemId) => childrenMap.get(itemId) ?? [],
  };

  return { dataLoader, rootChildren };
}

/** 给「正在选择 / 已选」场景：输入扁平 file 数组 */
export function buildTreeDataFromSelection(
  files: { name: string; relativePath: string; size: number }[],
): TreeData {
  return buildTreeFromEntries(files);
}

/** 给「传输中 / 历史」场景：file 含 fileId 用来匹配进度事件 */
export function buildTreeDataFromOffer(
  files: { fileId: number; name: string; relativePath: string; size: number }[],
): TreeData {
  return buildTreeFromEntries(files);
}
