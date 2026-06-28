import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import { File } from "expo-file-system";
import { useFocusEffect, useLocalSearchParams, useRouter } from "expo-router";
import * as Sharing from "expo-sharing";
import {
  Archive,
  ArchiveRestore,
  Copy,
  Database,
  ExternalLink,
  FileArchive,
  FileWarning,
  Share2,
  Trash2,
} from "lucide-react-native";
import { useCallback, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { MobileInboxContentKind } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { AppScreen, Surface } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { formatBytes } from "@/components/transfer/shared";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { errorMessage } from "@/lib/utils";
import { type InboxFileEntry, useInboxStore } from "@/stores/inbox-store";

export default function InboxDetailScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const { itemId } = useLocalSearchParams<{ itemId: string }>();
  const [deleteMode, setDeleteMode] = useState<"record" | "content" | null>(
    null,
  );
  const {
    detail,
    detailLoading,
    action,
    loadDetail,
    clearDetail,
    markOpened,
    archiveItem,
    deleteItem,
    markFileMissing,
  } = useInboxStore(
    useShallow((s) => ({
      detail: s.selectedDetail,
      detailLoading: s.detailLoading,
      action: s.action,
      loadDetail: s.loadDetail,
      clearDetail: s.clearDetail,
      markOpened: s.markOpened,
      archiveItem: s.archiveItem,
      deleteItem: s.deleteItem,
      markFileMissing: s.markFileMissing,
    })),
  );

  useFocusEffect(
    useCallback(() => {
      if (!itemId) return undefined;
      let cancelled = false;
      void (async () => {
        const loaded = await loadDetail(itemId);
        if (!cancelled && loaded) {
          void markOpened(itemId);
        }
      })();
      return () => {
        cancelled = true;
        clearDetail();
      };
    }, [itemId, loadDetail, markOpened, clearDetail]),
  );

  const archived = detail?.item.archivedAt != null;
  const title = detail?.item.title ?? t`收件箱详情`;
  const fileCount = detail?.files.length ?? 0;

  const performArchive = useCallback(async () => {
    if (!itemId || !detail) return;
    try {
      await archiveItem(itemId, !archived);
      toast.success(archived ? t`已取消归档` : t`已归档`);
      if (!archived) router.back();
    } catch (err) {
      toast.error(t`更新归档状态失败`, err);
    }
  }, [itemId, detail, archiveItem, archived, router, t]);

  const performDelete = useCallback(async () => {
    if (!itemId || !deleteMode) return;
    try {
      await deleteItem(itemId, { deleteLocalFiles: deleteMode === "content" });
      toast.success(
        deleteMode === "content" ? t`已删除记录和本地文件` : t`已删除记录`,
      );
      setDeleteMode(null);
      router.back();
    } catch (err) {
      toast.error(t`删除失败`, err);
    }
  }, [itemId, deleteMode, deleteItem, router, t]);

  const openOrShareFile = useCallback(
    async (file: InboxFileEntry) => {
      if (!itemId) return;
      try {
        await ensureAvailable(file);
        const available = await Sharing.isAvailableAsync();
        if (!available) {
          await Clipboard.setStringAsync(file.localPath);
          toast.info(t`系统分享不可用，已复制路径`);
          return;
        }
        await Sharing.shareAsync(file.localPath, {
          dialogTitle: t`打开或分享文件`,
        });
      } catch (err) {
        if (isMissingFileError(err)) {
          await markFileMissing(itemId, file.id, true);
          toast.error(t`文件已不在原位置`, file.localPath);
          return;
        }
        toast.error(t`打开失败`, err);
      }
    },
    [itemId, markFileMissing, t],
  );

  const copyFilePath = useCallback(
    async (path: string) => {
      await Clipboard.setStringAsync(path);
      toast.success(t`已复制路径`);
    },
    [t],
  );

  const transferSessionId = detail?.item.transferSessionId;
  const openTransfer = useCallback(() => {
    if (!transferSessionId) return;
    router.push({
      pathname: "/transfer/[sessionId]",
      params: { sessionId: transferSessionId },
    } as never);
  }, [router, transferSessionId]);

  const deleteTitle = useMemo(() => {
    if (deleteMode === "content") return t`删除记录和本地文件`;
    return t`删除这条收件箱记录`;
  }, [deleteMode, t]);

  return (
    <AppScreen
      scroll
      testID="inbox-detail-screen"
      contentClassName="gap-5 px-0 pt-0"
    >
      <SettingsHeader
        title={t`收件箱详情`}
        right={
          detail ? (
            <Pressable
              onPress={performArchive}
              accessibilityRole="button"
              accessibilityLabel={archived ? t`取消归档` : t`归档`}
              testID="inbox-detail-archive-button"
              className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70"
            >
              {archived ? (
                <ArchiveRestore color={colors.foreground} size={19} />
              ) : (
                <Archive color={colors.foreground} size={19} />
              )}
            </Pressable>
          ) : null
        }
      />

      <View className="gap-5 px-5">
        {detailLoading && !detail ? (
          <View className="items-center justify-center gap-3 py-20">
            <ActivityIndicator color={colors.mutedForeground} />
            <Text className="text-[12px] text-muted-foreground">
              <Trans>加载中</Trans>
            </Text>
          </View>
        ) : !detail ? (
          <Surface className="items-center gap-3 py-12">
            <FileArchive color={colors.mutedForeground} size={30} />
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>收件箱记录不存在</Trans>
            </Text>
            <Text className="text-center text-[12px] text-muted-foreground">
              <Trans>它可能已经被删除或归档隐藏。</Trans>
            </Text>
          </Surface>
        ) : (
          <>
            <Surface className="gap-4" testID="inbox-detail-summary">
              <View className="flex-row items-start gap-3">
                <View className="size-12 items-center justify-center rounded-xl bg-muted">
                  {detail.item.missing ? (
                    <FileWarning color={colors.destructive} size={22} />
                  ) : (
                    <Database color={colors.foreground} size={22} />
                  )}
                </View>
                <View className="min-w-0 flex-1 gap-1">
                  <Text className="text-[17px] font-semibold text-foreground">
                    {title}
                  </Text>
                  <Text className="text-[12px] text-muted-foreground">
                    {detail.item.sourceName}
                    {" · "}
                    {fileCount} <Trans>个文件</Trans>
                    {" · "}
                    {formatBytes(detail.item.totalSize)}
                  </Text>
                  <View className="mt-1 flex-row flex-wrap gap-1.5">
                    <ContentKindPill kind={detail.item.contentKind} />
                    {detail.item.missing ? (
                      <StatePill
                        tone="missing"
                        label={<Trans>文件缺失</Trans>}
                      />
                    ) : null}
                    {archived ? (
                      <StatePill tone="muted" label={<Trans>已归档</Trans>} />
                    ) : null}
                  </View>
                </View>
              </View>

              <View className="gap-2">
                <DetailRow
                  label={<Trans>接收时间</Trans>}
                  value={new Date(
                    Number(detail.item.receivedAt),
                  ).toLocaleString()}
                />
                <DetailRow
                  label={<Trans>来源设备</Trans>}
                  value={detail.item.sourcePeerId}
                  mono
                />
                {detail.item.rootPath ? (
                  <DetailRow
                    label={<Trans>保存位置</Trans>}
                    value={decodeURIComponent(detail.item.rootPath)}
                    mono
                  />
                ) : null}
                {detail.item.lastOpenedAt != null ? (
                  <DetailRow
                    label={<Trans>上次打开</Trans>}
                    value={new Date(
                      Number(detail.item.lastOpenedAt),
                    ).toLocaleString()}
                  />
                ) : null}
              </View>
            </Surface>

            {transferSessionId ? (
              <Pressable
                accessibilityRole="button"
                onPress={openTransfer}
                testID="inbox-detail-transfer-link"
                className="min-h-12 flex-row items-center justify-between gap-3 rounded-lg border border-border bg-card px-3.5 py-3 active:opacity-70"
              >
                <View className="min-w-0 flex-1">
                  <Text className="text-[13px] font-semibold text-foreground">
                    <Trans>查看传输诊断</Trans>
                  </Text>
                  <Text className="text-[11px] text-muted-foreground">
                    <Trans>跳转到活动详情</Trans>
                  </Text>
                </View>
                <ExternalLink color={colors.mutedForeground} size={17} />
              </Pressable>
            ) : null}

            <View className="gap-2.5" testID="inbox-detail-files">
              <Text className="text-[15px] font-semibold text-foreground">
                <Trans>文件</Trans>
              </Text>
              {detail.files.map((file, index) => (
                <FileRow
                  key={file.id}
                  file={file}
                  index={index}
                  onOpenShare={openOrShareFile}
                  onCopy={copyFilePath}
                />
              ))}
            </View>

            <View className="gap-2 pb-2">
              <ActionButton
                icon={archived ? ArchiveRestore : Archive}
                label={archived ? <Trans>取消归档</Trans> : <Trans>归档</Trans>}
                onPress={performArchive}
                disabled={action === "archive"}
                variant="secondary"
                testID="inbox-detail-archive-action"
              />
              <ActionButton
                icon={Trash2}
                label={<Trans>仅删除记录</Trans>}
                onPress={() => setDeleteMode("record")}
                disabled={action != null}
                variant="destructive"
                testID="inbox-detail-delete-record-button"
              />
              <ActionButton
                icon={Trash2}
                label={<Trans>删除记录和本地文件</Trans>}
                onPress={() => setDeleteMode("content")}
                disabled={action != null}
                variant="destructive"
                testID="inbox-detail-delete-content-button"
              />
            </View>
          </>
        )}
      </View>

      <ConfirmDialog
        open={deleteMode !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteMode(null);
        }}
        title={deleteTitle}
        description={
          deleteMode === "content" ? (
            <Trans>这会删除收件箱记录，并尝试删除本机保存的文件。</Trans>
          ) : (
            <Trans>这只删除收件箱索引，不会删除本地文件。</Trans>
          )
        }
        actionLabel={<Trans>删除</Trans>}
        destructive
        onAction={performDelete}
        contentTestID="inbox-delete-confirmation"
        actionTestID="inbox-delete-confirm-button"
        cancelTestID="inbox-delete-cancel-button"
      />
    </AppScreen>
  );
}

function FileRow({
  file,
  index,
  onOpenShare,
  onCopy,
}: {
  file: InboxFileEntry;
  index: number;
  onOpenShare: (file: InboxFileEntry) => void;
  onCopy: (path: string) => void;
}) {
  const colors = useThemeColors();
  return (
    <Surface className="gap-3" testID={`inbox-file-row-${index}`}>
      <View className="flex-row items-start gap-3">
        <View className="size-10 items-center justify-center rounded-xl bg-muted">
          {file.missing ? (
            <FileWarning color={colors.destructive} size={18} />
          ) : (
            <FileArchive color={colors.foreground} size={18} />
          )}
        </View>
        <View className="min-w-0 flex-1 gap-1">
          <View className="flex-row items-center gap-2">
            <Text
              className="min-w-0 flex-1 text-[13px] font-semibold text-foreground"
              numberOfLines={1}
            >
              {file.name}
            </Text>
            {file.missing ? (
              <StatePill tone="missing" label={<Trans>缺失</Trans>} />
            ) : null}
          </View>
          <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
            {file.relativePath}
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            {formatBytes(file.size)}
          </Text>
        </View>
      </View>
      <View className="flex-row gap-2">
        <MiniAction
          icon={Share2}
          label={<Trans>打开/分享</Trans>}
          onPress={() => onOpenShare(file)}
          disabled={file.missing}
          testID={`inbox-file-share-${index}`}
        />
        <MiniAction
          icon={Copy}
          label={<Trans>复制路径</Trans>}
          onPress={() => onCopy(file.localPath)}
          testID={`inbox-file-copy-${index}`}
        />
      </View>
    </Surface>
  );
}

function ContentKindPill({ kind }: { kind: MobileInboxContentKind }) {
  const label =
    kind === MobileInboxContentKind.Files ? (
      <Trans>文件</Trans>
    ) : kind === MobileInboxContentKind.Text ? (
      <Trans>文本</Trans>
    ) : kind === MobileInboxContentKind.Clipboard ? (
      <Trans>剪贴板</Trans>
    ) : (
      <Trans>组合内容</Trans>
    );
  return <StatePill tone="muted" label={label} />;
}

function StatePill({
  tone,
  label,
}: {
  tone: "missing" | "muted";
  label: React.ReactNode;
}) {
  return (
    <View
      className={
        tone === "missing"
          ? "rounded-full bg-destructive/15 px-2 py-0.5"
          : "rounded-full bg-muted px-2 py-0.5"
      }
    >
      <Text
        className={
          tone === "missing"
            ? "text-[10px] font-medium text-destructive"
            : "text-[10px] font-medium text-muted-foreground"
        }
      >
        {label}
      </Text>
    </View>
  );
}

function DetailRow({
  label,
  value,
  mono = false,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <View className="flex-row items-start justify-between gap-3">
      <Text className="text-[12px] text-muted-foreground">{label}</Text>
      <Text
        className={`min-w-0 flex-1 text-right text-[12px] text-foreground ${
          mono ? "font-mono" : ""
        }`}
        numberOfLines={3}
      >
        {value}
      </Text>
    </View>
  );
}

function ActionButton({
  icon: Icon,
  label,
  onPress,
  disabled,
  variant,
  testID,
}: {
  icon: typeof Archive;
  label: React.ReactNode;
  onPress: () => void;
  disabled?: boolean;
  variant: "secondary" | "destructive";
  testID?: string;
}) {
  const colors = useThemeColors();
  const destructive = variant === "destructive";
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      disabled={disabled}
      testID={testID}
      className={`min-h-11 flex-row items-center justify-center gap-2 rounded-xl px-3 py-2.5 active:opacity-70 disabled:opacity-50 ${
        destructive
          ? "border border-destructive/40 bg-destructive/10"
          : "border border-border bg-card"
      }`}
    >
      <Icon
        color={destructive ? colors.destructive : colors.foreground}
        size={17}
      />
      <Text
        className={`text-[13px] font-semibold ${
          destructive ? "text-destructive" : "text-foreground"
        }`}
      >
        {label}
      </Text>
    </Pressable>
  );
}

function MiniAction({
  icon: Icon,
  label,
  onPress,
  disabled,
  testID,
}: {
  icon: typeof Copy;
  label: React.ReactNode;
  onPress: () => void;
  disabled?: boolean;
  testID?: string;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      disabled={disabled}
      testID={testID}
      className="min-h-10 flex-1 flex-row items-center justify-center gap-1.5 rounded-lg bg-muted px-2 active:opacity-70 disabled:opacity-50"
    >
      <Icon color={colors.foreground} size={15} />
      <Text className="text-[12px] font-medium text-foreground">{label}</Text>
    </Pressable>
  );
}

function ensureAvailable(file: InboxFileEntry): void {
  if (file.missing) {
    throw new MissingFileError();
  }
  if (!file.localPath.startsWith("file://")) {
    return;
  }
  const localFile = new File(file.localPath);
  if (!localFile.exists) {
    throw new MissingFileError();
  }
}

class MissingFileError extends Error {
  constructor() {
    super("missing inbox file");
  }
}

function isMissingFileError(err: unknown): boolean {
  if (err instanceof MissingFileError) return true;
  return errorMessage(err).toLowerCase().includes("not found");
}
