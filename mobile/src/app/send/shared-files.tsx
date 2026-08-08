import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { useMemo } from "react";
import { Pressable } from "react-native";
import {
  FileBrowser,
  type FileBrowserActions,
  fromSelectedFiles,
} from "@/components/file-browser";
import { AppScreen, BottomActionBar } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { useShareStore } from "@/stores/share-store";

export default function SharedFilesScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const sharedFiles = useShareStore((state) => state.sharedFiles);
  const removeSharedBySourceId = useShareStore(
    (state) => state.removeSharedBySourceId,
  );
  const removeSharedDirectory = useShareStore(
    (state) => state.removeSharedDirectory,
  );
  const items = useMemo(() => fromSelectedFiles(sharedFiles), [sharedFiles]);
  const actions = useMemo<FileBrowserActions>(
    () => ({
      removeItem: (item) => {
        // 共享模型的 `sourceId` 是 `string | number`（收件箱那一路用行主键）。
        // 发送侧存的一律是 `file://` 来源串，转一次窄回 string。
        if (item.sourceId) removeSharedBySourceId(String(item.sourceId));
      },
      removeDirectory: removeSharedDirectory,
    }),
    [removeSharedBySourceId, removeSharedDirectory],
  );

  return (
    <AppScreen header={<SettingsHeader title={t`分享文件`} />} bare>
      <FileBrowser
        items={items}
        scope="send"
        actions={actions}
        title={<Trans>分享文件</Trans>}
        testID="share-files-browser"
      />
      <BottomActionBar testID="share-files-action-bar">
        <Pressable
          onPress={() => router.back()}
          accessibilityRole="button"
          accessibilityLabel={t`完成文件检查`}
          className="min-h-12 flex-1 items-center justify-center rounded-xl bg-primary active:opacity-70"
        >
          <Text className="text-[14px] font-semibold text-primary-foreground">
            <Trans>完成</Trans>
          </Text>
        </Pressable>
      </BottomActionBar>
    </AppScreen>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
