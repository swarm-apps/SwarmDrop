import { Trans } from "@lingui/react/macro";
import { Files } from "lucide-react-native";
import { useCallback, useMemo } from "react";
import { View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { usePreferencesStore } from "@/stores/preferences-store";
import { FileBrowserToolbar } from "./file-browser-toolbar";
import { FileGridView } from "./file-grid-view";
import { FileTreeView } from "./file-tree-view";
import type { FileBrowserProps, FileBrowserView } from "./types";

export function FileBrowser({
  items,
  scope,
  actions,
  title,
  contentHeader,
  contentFooter,
  listContext = "screen",
  testID = "file-browser",
  resetKey = "default",
  initialScrollIndex,
  onViewChange,
}: FileBrowserProps) {
  const colors = useThemeColors();
  const view = usePreferencesStore((state) => state.fileBrowserViews[scope]);
  const setFileBrowserView = usePreferencesStore(
    (state) => state.setFileBrowserView,
  );

  const totalSize = useMemo(
    () => items.reduce((total, item) => total + item.size, 0n),
    [items],
  );

  const changeView = useCallback(
    (next: FileBrowserView) => {
      setFileBrowserView(scope, next);
      onViewChange?.(next);
    },
    [onViewChange, scope, setFileBrowserView],
  );

  const listHeader = (
    <View>
      {contentHeader}
      <FileBrowserToolbar
        title={title}
        count={items.length}
        totalSize={totalSize}
        view={view}
        onViewChange={changeView}
        testID={`${testID}-toolbar`}
      />
    </View>
  );

  if (items.length === 0) {
    return (
      <View className="px-5" style={{ flex: 1 }} testID={testID}>
        {contentHeader}
        <FileBrowserToolbar
          title={title}
          count={0}
          totalSize={0n}
          view={view}
          onViewChange={changeView}
          testID={`${testID}-toolbar`}
        />
        <View
          className="flex-1 items-center justify-center gap-2 rounded-lg border border-dashed border-border"
          testID={`${testID}-empty`}
        >
          <View className="size-14 items-center justify-center rounded-full bg-muted">
            <Files size={24} color={colors.mutedForeground} />
          </View>
          <Text className="text-[13px] font-semibold text-foreground">
            <Trans>没有文件</Trans>
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            <Trans>添加文件后会显示在这里</Trans>
          </Text>
        </View>
      </View>
    );
  }

  return (
    <View style={{ flex: 1 }} testID={testID}>
      {view === "tree" ? (
        <FileTreeView
          key={`tree:${resetKey}`}
          items={items}
          actions={actions}
          listContext={listContext}
          header={listHeader}
          footer={contentFooter}
          testID={testID}
          initialScrollIndex={initialScrollIndex}
        />
      ) : (
        <FileGridView
          key={`grid:${resetKey}`}
          items={items}
          actions={actions}
          listContext={listContext}
          header={listHeader}
          footer={contentFooter}
          testID={testID}
          initialScrollIndex={initialScrollIndex}
        />
      )}
    </View>
  );
}
