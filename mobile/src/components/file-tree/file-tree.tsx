/**
 * FileTree —— RN 版文件树容器。
 *
 * 与桌面端 `file-tree.tsx` 同构：
 * - headless-tree 维护展开/折叠状态（跨平台、无 DOM 依赖）
 * - 虚拟滚动用 `@shopify/flash-list` 替代桌面的 @tanstack/react-virtual
 *
 * 两种模式：
 * - `select`：选择阶段，文件行 X 删除、目录行 X 删除子树
 * - `transfer`：进度阶段，按 fileId 匹配 progress 渲染状态
 */

import { syncDataLoaderFeature } from "@headless-tree/core";
import { useTree } from "@headless-tree/react";
import { Trans } from "@lingui/react/macro";
import { FlashList } from "@shopify/flash-list";
import { useCallback, useEffect, useMemo, useReducer } from "react";
import { View } from "react-native";
import type {
  MobileFileProgress,
  MobileTransferProgress,
} from "react-native-swarmdrop-core";
import { calcPercent, formatBytes } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import type { FileStatus, TreeDataLoader, TreeNodeData } from "./data";
import { FileTreeItem } from "./file-tree-item";
import { FolderRow } from "./folder-row";

interface FileTreeProps {
  mode: "select" | "transfer";
  dataLoader: TreeDataLoader;
  rootChildren: string[];
  totalCount: number;
  totalSize: number;
  /** transfer 模式下的实时进度 */
  progress?: MobileTransferProgress | null;
  /** transfer 模式下：已完成的 fileId 集合 */
  completedFileIds?: Set<number>;
  /** transfer 模式下：失败的 fileId 集合 */
  errorFileIds?: Set<number>;
  /** select 模式下删除文件/目录回调，参数是 relativePath（目录以 / 结尾） */
  onRemove?: (relativePath: string) => void;
  /** error 文件重试 */
  onRetryFile?: (fileId: number) => void;
  /** 文件行点击（详情页用：分享文件） */
  onFilePress?: (data: TreeNodeData) => void;
  /** 文件行长按（详情页用：复制路径） */
  onFileLongPress?: (data: TreeNodeData) => void;
  /** 是否显示头部（默认 true） */
  showHeader?: boolean;
  /**
   * 是否启用虚拟滚动（FlashList）。
   * - 默认 false：用 plain map 渲染，可嵌套在外层 ScrollView 中（详情页 / 发送页）
   * - true：用 FlashList 虚拟滚动，适合非常大的文件树（不要嵌套在 ScrollView 内）
   */
  virtualize?: boolean;
}

export function FileTree({
  mode,
  dataLoader,
  rootChildren,
  totalCount,
  totalSize,
  progress,
  completedFileIds,
  errorFileIds,
  onRemove,
  onRetryFile,
  onFilePress,
  onFileLongPress,
  showHeader = true,
  virtualize = false,
}: FileTreeProps) {
  const wrappedDataLoader = useMemo<TreeDataLoader>(
    () => ({
      getItem: dataLoader.getItem,
      getChildren: (itemId) =>
        itemId === "root" ? rootChildren : dataLoader.getChildren(itemId),
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

  // wrappedDataLoader 才是触发条件；tree 引用稳定（useTree 内部维护）。
  // biome-ignore lint/correctness/useExhaustiveDependencies: rebuildTree 只需 wrappedDataLoader 变化触发
  useEffect(() => {
    tree.rebuildTree();
  }, [wrappedDataLoader]);

  // headless-tree 的 expand/collapse 通过 mutate 内部 state 对象后回调 React 的
  // setState(state)。由于 state 引用未变，React 会 bail out（Object.is），导致
  // 文件夹点击不展开。这里在 onToggle 后显式 forceUpdate 触发 re-render，重新
  // 读取 tree.getItems() 的最新顺序。
  const [, forceUpdate] = useReducer((x: number) => x + 1, 0);

  const handleToggle = useCallback((item: (typeof items)[number]) => {
    if (item.isExpanded()) {
      item.collapse();
    } else {
      item.expand();
    }
    forceUpdate();
  }, []);

  // 一次性把 progress.files 转成 Map<fileId, FileProgress>，避免 renderRow 里
  // 每个文件 O(N) 线性查找两次（getFileStatus + getFileProgress）。
  const progressByFileId = useMemo(() => {
    const m = new Map<number, MobileFileProgress>();
    if (progress?.files) {
      for (const f of progress.files) m.set(f.fileId, f);
    }
    return m;
  }, [progress]);

  const items = tree.getItems();

  if (rootChildren.length === 0) return null;

  const renderRow = (item: (typeof items)[number]) => {
    const data = item.getItemData();
    const level = item.getItemMeta().level;

    if (data.type === "directory") {
      return (
        <FolderRow
          key={data.id}
          name={data.name}
          isExpanded={item.isExpanded()}
          fileCount={data.fileCount ?? 0}
          totalSize={data.size}
          level={level}
          mode={mode}
          onToggle={() => handleToggle(item)}
          onRemove={
            mode === "select" && onRemove ? () => onRemove(data.id) : undefined
          }
        />
      );
    }

    const fileStatus = getFileStatus(
      data,
      mode,
      progressByFileId,
      completedFileIds,
      errorFileIds,
    );

    return (
      <FileTreeItem
        key={data.id}
        name={data.name}
        size={data.size}
        variant={fileStatus}
        level={level}
        progress={getFileProgress(data, progressByFileId)}
        onRemove={
          mode === "select" && onRemove ? () => onRemove(data.id) : undefined
        }
        onRetry={
          fileStatus === "error" && onRetryFile && data.fileId != null
            ? () => onRetryFile(data.fileId as number)
            : undefined
        }
        onPress={onFilePress ? () => onFilePress(data) : undefined}
        onLongPress={onFileLongPress ? () => onFileLongPress(data) : undefined}
      />
    );
  };

  return (
    <View className={virtualize ? "flex-1 gap-2" : "gap-2"}>
      {showHeader ? (
        <View className="flex-row items-center justify-between">
          <Text className="text-sm font-medium text-foreground">
            {mode === "select" ? <Trans>已选文件</Trans> : <Trans>文件</Trans>}
          </Text>
          <Text className="text-xs text-muted-foreground">
            {mode === "select" ? (
              <Trans>
                共 {totalCount} 项 · {formatBytes(totalSize)}
              </Trans>
            ) : (
              `${progress?.completedFiles ?? 0}/${totalCount}`
            )}
          </Text>
        </View>
      ) : null}

      <View
        className="overflow-hidden rounded-xl border border-border bg-card"
        style={virtualize ? { minHeight: 120 } : undefined}
      >
        {virtualize ? (
          <FlashList
            data={items}
            extraData={progress}
            keyExtractor={(item) => item.getItemData().id}
            renderItem={({ item }) => renderRow(item)}
          />
        ) : (
          <View className="py-1">{items.map(renderRow)}</View>
        )}
      </View>
    </View>
  );
}

function getFileStatus(
  data: TreeNodeData,
  mode: "select" | "transfer",
  progressByFileId: Map<number, MobileFileProgress>,
  completedFileIds?: Set<number>,
  errorFileIds?: Set<number>,
): FileStatus {
  if (mode === "select") return "select";
  if (data.fileId == null) return "waiting";

  if (errorFileIds?.has(data.fileId)) return "error";
  if (completedFileIds?.has(data.fileId)) return "completed";

  const fileProgress = progressByFileId.get(data.fileId);
  if (fileProgress?.status === "completed") return "completed";
  if (fileProgress?.status === "transferring") return "transferring";
  return "waiting";
}

function getFileProgress(
  data: TreeNodeData,
  progressByFileId: Map<number, MobileFileProgress>,
): number {
  if (data.fileId == null) return 0;
  const fileProgress = progressByFileId.get(data.fileId);
  if (!fileProgress) return 0;
  return calcPercent(fileProgress.transferred, fileProgress.size);
}
