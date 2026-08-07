/**
 * L1 的中立嵌套树 → `@headless-tree` 要的扁平数据源。
 *
 * L1（`buildFileBrowserTree`）刻意只产出嵌套树：移动端要的是 FlashList 的扁平行，
 * 桌面/Web 要的是 headless-tree 的 `{ getItem, getChildren }` + 一个虚拟根。把其中
 * 任一形态烤进 L1，另一端就得反向拆解一遍。这个文件就是那笔差额。
 */

import {
  type FileBrowserTree,
  type FileBrowserTreeNode,
} from "@swarmdrop/shared-view";

/** headless-tree 的根 id。它要求有一个统辖全部顶层节点的虚拟根。 */
export const TREE_ROOT_ID = "root";

/**
 * 树里的一个节点。**根是虚拟的**（L1 的树没有根节点，只有 `roots` 数组），
 * 所以这里补一个 `type: "directory"` 的壳，让 headless-tree 的
 * `isItemFolder` / `getItemName` 有统一的形状可读。
 */
export type TreeNodeData =
  | FileBrowserTreeNode
  | {
      type: "directory";
      id: typeof TREE_ROOT_ID;
      name: string;
      relativePath: "";
      depth: -1;
      fileCount: number;
      size: number;
      children: FileBrowserTreeNode[];
    };

export interface HeadlessTreeData {
  /** 直接喂给 `useTree({ dataLoader })`。 */
  dataLoader: {
    getItem: (itemId: string) => TreeNodeData;
    getChildren: (itemId: string) => string[];
  };
  /** 所有目录 id，供初始化展开态。 */
  directoryIds: ReadonlySet<string>;
}

export function toHeadlessTreeData(tree: FileBrowserTree): HeadlessTreeData {
  const nodes = new Map<string, TreeNodeData>();
  const children = new Map<string, string[]>();

  const visit = (list: readonly FileBrowserTreeNode[]): string[] => {
    const ids: string[] = [];
    for (const node of list) {
      nodes.set(node.id, node);
      ids.push(node.id);
      if (node.type === "directory") {
        children.set(node.id, visit(node.children));
      }
    }
    return ids;
  };

  const root: TreeNodeData = {
    type: "directory",
    id: TREE_ROOT_ID,
    name: TREE_ROOT_ID,
    relativePath: "",
    depth: -1,
    fileCount: tree.totalCount,
    size: tree.totalSize,
    children: [...tree.roots],
  };
  nodes.set(TREE_ROOT_ID, root);
  children.set(TREE_ROOT_ID, visit(tree.roots));

  return {
    dataLoader: {
      // 取不到时回落到根而不是抛错：headless-tree 在重建期间可能查到已经不存在的 id
      // （上一棵树的节点），抛错会把整个列表打挂。
      getItem: (itemId) => nodes.get(itemId) ?? root,
      getChildren: (itemId) => children.get(itemId) ?? [],
    },
    directoryIds: tree.directoryIds,
  };
}

/**
 * 节点的显示名。
 *
 * 目录节点自己带 `name`，文件节点的名字在 `item.name` 里——L1 刻意不给文件节点冗余一份
 * （它会与 `item.name` 有漂移的可能）。headless-tree 的 `getItemName` 要一个统一入口，
 * 这里就是。
 */
export function nodeName(node: TreeNodeData): string {
  return node.type === "file" ? node.item.name : node.name;
}
