/**
 * FileTree
 * 文件树容器组件 — 集成 headless-tree + @tanstack/react-virtual
 * 支持两种模式：select（文件选择）和 transfer（传输进度）
 */

import { useRef, useMemo, useEffect } from "react";
import { useTree } from "@headless-tree/react";
import { syncDataLoaderFeature } from "@headless-tree/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Trans } from "@lingui/react/macro";
import { formatFileSize } from "@/lib/format";
import type { TreeDataLoader, TreeNodeData, FileStatus } from "./data";
import type { TransferProgressEvent } from "@/commands/transfer";
import { FileTreeItem } from "./file-tree-item";
import { FolderRow } from "./folder-row";

interface FileTreeProps {
  /** 显示模式 */
  mode: "select" | "transfer";
  /** headless-tree 数据加载器 */
  dataLoader: TreeDataLoader;
  /** 根级子节点 ID */
  rootChildren: string[];
  /** 文件总数 */
  totalCount: number;
  /** 总大小 */
  totalSize: number;
  /** 传输进度（transfer 模式） */
  progress?: TransferProgressEvent | null;
  /** 已完成的 fileId 集合（transfer 模式） */
  completedFileIds?: Set<number>;
  /** 失败的 fileId 集合（transfer 模式） */
  errorFileIds?: Set<number>;
  /** 删除文件回调（select 模式） */
  onRemoveFile?: (absolutePath: string) => void;
  /** 重试文件回调（transfer 模式） */
  onRetryFile?: (fileId: number) => void;
  /** 是否显示头部（默认 true） */
  showHeader?: boolean;
}

/** 行高（px） */
const ROW_HEIGHT = 40;
/** 缩进步长（px） */
const INDENT_SIZE = 22;

export function FileTree({
  mode,
  dataLoader,
  rootChildren,
  totalCount,
  totalSize,
  progress,
  completedFileIds,
  errorFileIds,
  onRemoveFile,
  onRetryFile,
  showHeader = true,
}: FileTreeProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // 包装 dataLoader，确保根节点 children 正确
  const wrappedDataLoader = useMemo(
    () => ({
      getItem: dataLoader.getItem,
      getChildren: (itemId: string) => {
        if (itemId === "root") return rootChildren;
        return dataLoader.getChildren(itemId);
      },
    }),
    [dataLoader, rootChildren],
  );

  const tree = useTree<TreeNodeData>({
    rootItemId: "root",
    dataLoader: wrappedDataLoader,
    getItemName: (item) => item.getItemData().name,
    isItemFolder: (item) => item.getItemData().type === "directory",
    features: [syncDataLoaderFeature],
  });

  // 数据源变更时通知 headless-tree 重新读取
  useEffect(() => {
    tree.rebuildTree();
  }, [tree, wrappedDataLoader]);

  const items = tree.getItems();

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  if (rootChildren.length === 0) return null;

  return (
    <div className="flex flex-col gap-2 md:min-h-0 md:flex-1">
      {/* 头部 */}
      {showHeader && (
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-medium text-foreground">
            {mode === "select" ? (
              <Trans>已选文件</Trans>
            ) : (
              <Trans>文件</Trans>
            )}
          </h3>
          <span className="text-xs text-muted-foreground">
            {mode === "select" ? (
              <Trans>
                共 {totalCount} 项 · {formatFileSize(totalSize)}
              </Trans>
            ) : (
              <Trans>
                {progress?.completedFiles ?? 0}/{totalCount}
              </Trans>
            )}
          </span>
        </div>
      )}

      {/* 树列表（虚拟滚动） */}
      <div
        ref={scrollRef}
        className="max-h-64 overflow-auto rounded-lg border bg-card p-1.5 md:max-h-none md:min-h-0 md:flex-1"
      >
        <div
          className="relative w-full"
          style={{ height: `${virtualizer.getTotalSize()}px` }}
        >
          {virtualizer.getVirtualItems().map((virtualItem) => {
            const item = items[virtualItem.index];
            const data = item.getItemData();
            const meta = item.getItemMeta();
            const level = meta.level;

            // 缩进引导线（level > 0 时显示，level 0 是可见根级）
            const guides =
              level > 0
                ? Array.from({ length: level }, (_, i) => (
                    <div
                      key={`g${i}`}
                      className="absolute top-0 h-full w-px bg-border/40"
                      style={{ left: `${i * INDENT_SIZE + 16}px` }}
                    />
                  ))
                : null;

            // 文件行
            const fileStatus =
              data.type !== "directory"
                ? getFileStatus(data, mode, progress, completedFileIds, errorFileIds)
                : undefined;

            // 共享的 onRemove 回调
            const onRemove =
              mode === "select" && onRemoveFile && data.absolutePath
                ? () => onRemoveFile(data.absolutePath!)
                : undefined;

            return (
              <div
                key={data.id}
                className="absolute left-0 top-0 w-full"
                style={{
                  height: `${virtualItem.size}px`,
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                {guides}
                {data.type === "directory" ? (
                  <FolderRow
                    name={data.name}
                    isExpanded={item.isExpanded()}
                    fileCount={data.fileCount ?? 0}
                    totalSize={data.size}
                    level={level}
                    mode={mode}
                    onToggle={() => {
                      if (item.isExpanded()) {
                        item.collapse();
                      } else {
                        item.expand();
                      }
                    }}
                    onRemove={onRemove}
                  />
                ) : (
                  <FileTreeItem
                    name={data.name}
                    size={data.size}
                    variant={fileStatus!}
                    level={level}
                    progress={getFileProgress(data, progress)}
                    onRemove={onRemove}
                    onRetry={
                      fileStatus === "error" && onRetryFile && data.fileId != null
                        ? () => onRetryFile(data.fileId!)
                        : undefined
                    }
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

/** 根据传输状态确定文件的显示状态 */
function getFileStatus(
  data: TreeNodeData,
  mode: string,
  progress?: TransferProgressEvent | null,
  completedFileIds?: Set<number>,
  errorFileIds?: Set<number>,
): FileStatus {
  if (mode === "select") return "select";
  if (data.fileId == null) return "waiting";

  if (errorFileIds?.has(data.fileId)) return "error";
  if (completedFileIds?.has(data.fileId)) return "completed";

  // 从 per-file 进度中查找状态
  const fileProgress = progress?.files?.find((f) => f.fileId === data.fileId);
  if (fileProgress) {
    if (fileProgress.status === "completed") return "completed";
    if (fileProgress.status === "transferring") return "transferring";
  }
  return "waiting";
}

/** 获取文件传输进度百分比 */
function getFileProgress(
  data: TreeNodeData,
  progress?: TransferProgressEvent | null,
): number {
  if (data.fileId == null || !progress?.files) return 0;
  const fileProgress = progress.files.find((f) => f.fileId === data.fileId);
  if (!fileProgress || fileProgress.size <= 0) return 0;
  return (fileProgress.transferred / fileProgress.size) * 100;
}
