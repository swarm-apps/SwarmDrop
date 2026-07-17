import { BottomSheetFlatList } from "@gorhom/bottom-sheet";
import { FlashList } from "@shopify/flash-list";
import { useCallback, useEffect, useMemo, useState } from "react";
import { View } from "react-native";
import { FileRow } from "./file-row";
import { FolderRow } from "./folder-row";
import {
  buildFileBrowserTree,
  type FileBrowserTreeRow,
  flattenVisibleNodes,
} from "./tree-data";
import type {
  FileBrowserActions,
  FileBrowserItem,
  FileBrowserListContext,
} from "./types";

interface FileTreeViewProps {
  items: FileBrowserItem[];
  actions?: FileBrowserActions;
  listContext: FileBrowserListContext;
  header: React.ReactElement;
  footer?: React.ReactElement | null;
  testID: string;
  initialScrollIndex?: number;
}

const CONTENT_STYLE = {
  paddingHorizontal: 20,
  paddingBottom: 24,
} as const;

export function FileTreeView({
  items,
  actions,
  listContext,
  header,
  footer,
  testID,
  initialScrollIndex,
}: FileTreeViewProps) {
  const tree = useMemo(() => buildFileBrowserTree(items), [items]);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    setExpandedIds((current) => {
      const next = new Set(
        [...current].filter((id) => tree.directoryIds.has(id)),
      );
      return next.size === current.size ? current : next;
    });
  }, [tree.directoryIds]);

  const rows = useMemo(
    () => flattenVisibleNodes(tree, expandedIds),
    [expandedIds, tree],
  );

  const toggleDirectory = useCallback((id: string) => {
    setExpandedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const renderItem = useCallback(
    ({ item, index }: { item: FileBrowserTreeRow; index: number }) => {
      if (item.type === "directory") {
        return (
          <FolderRow
            id={item.id}
            name={item.name}
            relativePath={item.relativePath}
            depth={item.depth}
            fileCount={item.fileCount}
            size={item.size}
            expanded={expandedIds.has(item.id)}
            actions={actions}
            onToggle={toggleDirectory}
            testID={`${testID}-directory-${index}`}
          />
        );
      }
      return (
        <FileRow
          item={item.item}
          depth={item.depth}
          actions={actions}
          testID={`${testID}-file-${index}`}
        />
      );
    },
    [actions, expandedIds, testID, toggleDirectory],
  );

  const keyExtractor = useCallback((row: FileBrowserTreeRow) => row.id, []);
  const getItemType = useCallback((row: FileBrowserTreeRow) => row.type, []);

  if (listContext === "bottom-sheet") {
    return (
      <BottomSheetFlatList
        style={{ flex: 1 }}
        data={rows}
        initialScrollIndex={initialScrollIndex}
        renderItem={renderItem}
        keyExtractor={keyExtractor}
        ListHeaderComponent={header}
        ListFooterComponent={footer}
        contentContainerStyle={CONTENT_STYLE}
        keyboardShouldPersistTaps="handled"
        testID={`${testID}-tree-list`}
      />
    );
  }

  return (
    <View style={{ flex: 1 }}>
      <FlashList
        style={{ flex: 1 }}
        data={rows}
        initialScrollIndex={initialScrollIndex}
        renderItem={renderItem}
        keyExtractor={keyExtractor}
        getItemType={getItemType}
        ListHeaderComponent={header}
        ListFooterComponent={footer}
        contentContainerStyle={CONTENT_STYLE}
        keyboardShouldPersistTaps="handled"
        testID={`${testID}-tree-list`}
      />
    </View>
  );
}
