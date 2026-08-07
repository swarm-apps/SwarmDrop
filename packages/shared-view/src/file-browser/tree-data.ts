/**
 * 由扁平 `FileBrowserItem[]` 建目录树 —— 三端共用的**核心算法**。
 *
 * 只做「按 `relativePath` 派生目录层级、目录递归累计 size/fileCount、目录优先排序」
 * 这一件事，产出**中立的嵌套树**（见 `types.ts` 的 `FileBrowserTree`）。
 *
 * **刻意不迁就任何一端的消费形态**：桌面要的是 `@headless-tree` 的扁平 `Map` +
 * `dataLoader`，移动要的是 FlashList 的扁平行。那两种投影分别住在各自的表现层——
 * 把其中一种烤进这里，另一端就得反向拆解一遍。
 */

import { normalizeRelativePath } from "./identity";
import type {
  FileBrowserDirectoryNode,
  FileBrowserItem,
  FileBrowserTree,
  FileBrowserTreeNode,
  FileBrowserTreeRow,
} from "./types";

export function buildFileBrowserTree(
  items: readonly FileBrowserItem[],
): FileBrowserTree {
  // 根不是一个可展示的节点（没有名字），只用来收集顶层子节点与总计。
  const root: FileBrowserDirectoryNode = {
    type: "directory",
    id: "dir:",
    name: "",
    relativePath: "",
    depth: -1,
    fileCount: 0,
    size: 0,
    children: [],
  };
  const directories = new Map<string, FileBrowserDirectoryNode>([["", root]]);

  for (const item of items) {
    const relativePath = normalizeRelativePath(item.relativePath, item.name);
    const segments = relativePath.split("/");
    let parent = root;
    let currentPath = "";

    // 逐段建目录。同一路径只建一次，后续文件复用同一个节点。
    //
    // 计数与求和在**同一趟**里沿祖先链累加——目录行显示的是「这个目录下一共多少个文件、
    // 多大」，含所有后代，所以每经过一个祖先就加一次。此前是建完再走第二趟，同一条链遍历
    // 两遍、还多做一轮 Map 查找；而第一趟手上本来就握着那个节点。
    for (const segment of segments.slice(0, -1)) {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;
      let directory = directories.get(currentPath);
      if (!directory) {
        directory = {
          type: "directory",
          id: `dir:${currentPath}/`,
          name: segment,
          // 带尾斜杠：目录类操作靠它做前缀匹配，没有斜杠时 `a/b` 会误匹配 `a/bc`。
          relativePath: `${currentPath}/`,
          depth: currentPath.split("/").length - 1,
          fileCount: 0,
          size: 0,
          children: [],
        };
        directories.set(currentPath, directory);
        parent.children.push(directory);
      }
      directory.fileCount += 1;
      directory.size += item.size;
      parent = directory;
    }

    parent.children.push({
      type: "file",
      id: `file:${item.id}`,
      depth: segments.length - 1,
      // 回写归一后的路径：树里的节点与 item 自报的路径必须一致，否则「按路径找节点」会落空。
      item: { ...item, relativePath },
    });

    root.fileCount += 1;
    root.size += item.size;
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
    totalSize: root.size,
    totalCount: root.fileCount,
  };
}

/**
 * 目录优先，其次按名称。
 *
 * `numeric: true` 让 `file2` 排在 `file10` 前面（否则字典序会给出 `file10 < file2`）；
 * `sensitivity: "base"` 让大小写不影响顺序。同名时用 id 兜底，保证排序稳定。
 */
function compareTreeNodes(
  a: FileBrowserTreeNode,
  b: FileBrowserTreeNode,
): number {
  if (a.type !== b.type) return a.type === "directory" ? -1 : 1;
  const aName = a.type === "directory" ? a.name : a.item.name;
  const bName = b.type === "directory" ? b.name : b.item.name;
  return NAME_COLLATOR.compare(aName, bName) || a.id.localeCompare(b.id);
}

/**
 * 复用一个 collator 而不是每次 `localeCompare(name, undefined, {...})`：带 options 的那个
 * 形态每次调用都要重新解析选项并查一次 collator，而排序在一个几百文件的会话里每帧要跑
 * 上千次比较。
 */
const NAME_COLLATOR = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

/**
 * 把树摊成「当前可见的行」——展开的目录才递归进去。
 *
 * 移动端的 FlashList 直接吃它。桌面用 `@headless-tree`，那边有自己的可见性计算，
 * 用不到这个函数，但它是纯逻辑，放在 L1 不增加任何一端的负担。
 */
export function flattenVisibleNodes(
  tree: FileBrowserTree,
  expandedIds: ReadonlySet<string>,
): FileBrowserTreeRow[] {
  const rows: FileBrowserTreeRow[] = [];
  const visit = (nodes: readonly FileBrowserTreeNode[]) => {
    for (const node of nodes) {
      if (node.type === "file") {
        rows.push(node);
        continue;
      }
      // 目录行剥掉 `children` 再入列——理由见 `FileBrowserTreeRow` 的说明。
      const { children, ...row } = node;
      rows.push(row);
      if (expandedIds.has(node.id)) visit(children);
    }
  };
  visit(tree.roots);
  return rows;
}
