import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { useMemo } from "react";
import { Pressable } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import {
  FileBrowser,
  type FileBrowserActions,
  fromSelectedFiles,
} from "@/components/file-browser";
import { BottomActionBar } from "@/components/mobile/screen";
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
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`分享文件`} />
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
    </SafeAreaView>
  );
}
