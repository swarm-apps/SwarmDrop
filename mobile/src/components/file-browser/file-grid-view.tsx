import { BottomSheetFlatList } from "@gorhom/bottom-sheet";
import { FlashList } from "@shopify/flash-list";
import { useCallback, useState } from "react";
import { type LayoutChangeEvent, View } from "react-native";
import { FileCard } from "./file-card";
import type {
  FileBrowserActions,
  FileBrowserItem,
  FileBrowserListContext,
} from "./types";

interface FileGridViewProps {
  items: FileBrowserItem[];
  actions?: FileBrowserActions;
  listContext: FileBrowserListContext;
  header: React.ReactElement;
  footer?: React.ReactElement | null;
  testID: string;
  initialScrollIndex?: number;
}

const CONTENT_STYLE = {
  paddingHorizontal: 16,
  paddingBottom: 24,
} as const;

export function FileGridView({
  items,
  actions,
  listContext,
  header,
  footer,
  testID,
  initialScrollIndex,
}: FileGridViewProps) {
  const [columns, setColumns] = useState(2);
  const handleLayout = useCallback((event: LayoutChangeEvent) => {
    const width = event.nativeEvent.layout.width;
    const next = width < 360 ? 1 : width < 640 ? 2 : width < 900 ? 3 : 4;
    setColumns((current) => (current === next ? current : next));
  }, []);

  const renderItem = useCallback(
    ({ item, index }: { item: FileBrowserItem; index: number }) => (
      <FileCard
        item={item}
        actions={actions}
        testID={`${testID}-file-${index}`}
      />
    ),
    [actions, testID],
  );
  const keyExtractor = useCallback((item: FileBrowserItem) => item.id, []);
  const listKey = `${testID}-grid-${columns}`;

  return (
    <View style={{ flex: 1 }} onLayout={handleLayout}>
      {listContext === "bottom-sheet" ? (
        <BottomSheetFlatList
          style={{ flex: 1 }}
          key={listKey}
          data={items}
          initialScrollIndex={initialScrollIndex}
          numColumns={columns}
          renderItem={renderItem}
          keyExtractor={keyExtractor}
          ListHeaderComponent={header}
          ListFooterComponent={footer}
          contentContainerStyle={CONTENT_STYLE}
          keyboardShouldPersistTaps="handled"
          testID={`${testID}-grid-list`}
        />
      ) : (
        <FlashList
          style={{ flex: 1 }}
          key={listKey}
          data={items}
          initialScrollIndex={initialScrollIndex}
          numColumns={columns}
          renderItem={renderItem}
          keyExtractor={keyExtractor}
          ListHeaderComponent={header}
          ListFooterComponent={footer}
          contentContainerStyle={CONTENT_STYLE}
          keyboardShouldPersistTaps="handled"
          testID={`${testID}-grid-list`}
        />
      )}
    </View>
  );
}
