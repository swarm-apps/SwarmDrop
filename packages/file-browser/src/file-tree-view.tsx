import { useCallback, useEffect, useMemo, useRef } from "react";
import { syncDataLoaderFeature } from "@headless-tree/core";
import { useTree } from "@headless-tree/react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { buildFileBrowserTree } from "@swarmdrop/shared-view";
import { FileRow } from "./file-row";
import { FolderRow } from "./folder-row";
import { TREE_ROOT_ID, nodeName, toHeadlessTreeData } from "./headless-tree-adapter";
import type { FileBrowserItem } from "@swarmdrop/shared-view";
import type { FileBrowserActions } from "./types";

const ROW_HEIGHT = 40;
const INDENT_SIZE = 22;

/**
 * 树的**拓扑签名**。
 *
 * 树的形状只由「有哪些文件、各自叫什么、多大、在哪」决定；传输过程中每秒变的是
 * `status` / `progress`，它们不影响任何一个节点的存在与位置。而 `items` 数组每 tick 都是
 * 新引用（`itemsFromProjection` 重建全部条目），拿它当 memo 依赖就等于每秒重建一次
 * 恒定不变的结果：全量建树（Map + 节点对象 + 每个目录一次 sort）→ 两张投影 Map →
 * headless-tree 全量 `rebuildTree()` → 所有可见行重渲染。
 *
 * 用签名当依赖之后，那条链只在**真的增删改文件**时跑。
 */
function topologyKey(items: readonly FileBrowserItem[]): string {
  let key = "";
  for (const item of items) {
    key += `${item.id}${item.relativePath}${item.size}`;
  }
  return key;
}

export function FileTreeView({
  items,
  actions,
}: {
  items: FileBrowserItem[];
  actions?: FileBrowserActions;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const shape = topologyKey(items);
  // L1 只给中立嵌套树，headless-tree 要的扁平 dataLoader 由 L2 投影（见 D1 的分界）。
  //
  // 依赖是 `shape` 不是 `items`：见 `topologyKey`。代价是树里那份 `node.item` 会停在建树
  // 那一刻的快照，所以下面渲染时**不读它**，改从 `liveItems` 查当前值。
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const treeData = useMemo(
    () => toHeadlessTreeData(buildFileBrowserTree(itemsRef.current)),
    [shape],
  );
  /** 展示 ID → 当前条目。行渲染读它而不是树里的快照，`status` / `progress` 才跟得上。 */
  const liveItems = useMemo(
    () => new Map(items.map((item) => [item.id, item])),
    [items],
  );

  const tree = useTree({
    rootItemId: TREE_ROOT_ID,
    dataLoader: treeData.dataLoader,
    getItemName: (item) => nodeName(item.getItemData()),
    isItemFolder: (item) => item.getItemData().type === "directory",
    features: [syncDataLoaderFeature],
  });

  // 首次挂载时 `useTree` 内部已经 rebuild 过一次，这里再跑就是同一趟全量重扫跑两遍。
  const built = useRef(false);
  useEffect(() => {
    if (!built.current) {
      built.current = true;
      return;
    }
    tree.rebuildTree();
  }, [tree, treeData]);

  const visibleItems = tree.getItems();
  const virtualizer = useVirtualizer({
    count: visibleItems.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  // 稳定引用：`FolderRow` 是 memo 的，每行一个新箭头函数会把它整片打穿。
  const toggle = useCallback((itemId: string) => {
    const target = tree.getItemInstance(itemId);
    if (target.isExpanded()) target.collapse();
    else target.expand();
  }, [tree]);

  return (
    <div
      ref={scrollRef}
      data-testid="file-browser-tree"
      className="min-h-0 flex-1 overflow-auto rounded-[14px] bg-foreground/[0.025] p-1.5 shadow-[inset_0_1px_0_rgba(255,255,255,0.4)] dark:bg-white/[0.035] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]"
    >
      <div
        className="relative w-full"
        style={{ height: `${virtualizer.getTotalSize()}px` }}
      >
        {virtualizer.getVirtualItems().map((virtualItem) => {
          const treeItem = visibleItems[virtualItem.index];
          const node = treeItem.getItemData();
          const level = treeItem.getItemMeta().level;

          return (
            <div
              key={node.id}
              className="absolute left-0 top-0 w-full"
              style={{
                height: `${virtualItem.size}px`,
                transform: `translateY(${virtualItem.start}px)`,
              }}
            >
              {level > 0 &&
                Array.from({ length: level }, (_, index) => (
                  <span
                    key={index}
                    aria-hidden
                    className="pointer-events-none absolute top-0 h-full w-px bg-border/35"
                    style={{ left: `${index * INDENT_SIZE + 15}px` }}
                  />
                ))}
              {node.type === "directory" ? (
                <FolderRow
                  node={node}
                  level={level}
                  expanded={treeItem.isExpanded()}
                  onToggle={toggle}
                  actions={actions}
                />
              ) : (
                <FileRow
                  item={liveItems.get(node.item.id) ?? node.item}
                  level={level}
                  actions={actions}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
