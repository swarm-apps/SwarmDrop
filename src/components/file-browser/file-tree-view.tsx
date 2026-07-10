import { useEffect, useMemo, useRef } from "react";
import { syncDataLoaderFeature } from "@headless-tree/core";
import { useTree } from "@headless-tree/react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { buildFileBrowserTree } from "./tree-data";
import { FileRow } from "./file-row";
import { FolderRow } from "./folder-row";
import type { FileBrowserActions, FileBrowserItem } from "./types";

const ROW_HEIGHT = 40;
const INDENT_SIZE = 22;

export function FileTreeView({
  items,
  actions,
}: {
  items: FileBrowserItem[];
  actions?: FileBrowserActions;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const treeData = useMemo(() => buildFileBrowserTree(items), [items]);
  const tree = useTree({
    rootItemId: "root",
    dataLoader: treeData.dataLoader,
    getItemName: (item) => item.getItemData().name,
    isItemFolder: (item) => item.getItemData().type === "directory",
    features: [syncDataLoaderFeature],
  });

  useEffect(() => {
    tree.rebuildTree();
  }, [tree, treeData]);

  const visibleItems = tree.getItems();
  const virtualizer = useVirtualizer({
    count: visibleItems.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

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
                  onToggle={() => treeItem.isExpanded() ? treeItem.collapse() : treeItem.expand()}
                  onRemove={actions?.onRemove}
                />
              ) : node.item ? (
                <FileRow item={node.item} level={level} actions={actions} />
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
}
