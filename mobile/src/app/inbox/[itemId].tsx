import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  BottomSheetModal,
  BottomSheetView,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import { File } from "expo-file-system";
import { useFocusEffect, useLocalSearchParams, useRouter } from "expo-router";
import * as Sharing from "expo-sharing";
import {
  Archive,
  ArchiveRestore,
  CalendarClock,
  ChevronDown,
  ChevronUp,
  ClipboardList,
  Copy,
  Database,
  ExternalLink,
  FileArchive,
  FileText,
  FileWarning,
  FolderOpen,
  HardDrive,
  Image as ImageIcon,
  type LucideIcon,
  MoreHorizontal,
  Package,
  Share2,
  Smartphone,
  Tag,
  Trash2,
  Video,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Image,
  Pressable,
  ScrollView,
  StyleSheet,
  View,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import {
  MobileInboxContentKind,
  type MobileInboxItemDetail,
  MobileInboxSourceKind,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { AppScreen, Surface } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { formatBytes, formatRelativeTime } from "@/components/transfer/shared";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { type InboxFileEntry, useInboxStore } from "@/stores/inbox-store";

export default function InboxDetailScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const insets = useSafeAreaInsets();
  const actionsSheetRef = useRef<BottomSheetModal>(null);
  const { itemId } = useLocalSearchParams<{ itemId: string }>();
  const [deleteMode, setDeleteMode] = useState<"record" | "content" | null>(
    null,
  );
  const [detailsExpanded, setDetailsExpanded] = useState(false);
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
      };
    }, [itemId, loadDetail, markOpened]),
  );

  // 仅在真正离开详情页（卸载）时清空，而非失焦——否则跳转「传输诊断」再返回会因
  // selectedDetail 被清空而触发整屏「加载中」reload 闪烁。重新聚焦会刷新数据，但旧详情
  // 仍在，不会闪烁。
  useEffect(() => {
    return () => clearDetail();
  }, [clearDetail]);

  const archived = detail?.item.archivedAt != null;
  const title = detail?.item.title ?? t`收件箱详情`;
  const fileCount = detail?.files.length ?? 0;
  const primaryFile = detail?.files.length === 1 ? detail.files[0] : null;

  const performArchive = useCallback(async () => {
    if (!itemId || !detail) return;
    try {
      await archiveItem(itemId, !archived);
      toast.success(archived ? t`已取消归档` : t`已归档`);
    } catch (err) {
      toast.error(t`更新归档状态失败`, err);
    }
  }, [itemId, detail, archiveItem, archived, t]);

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
        if (isMissingFileError(err, file)) {
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

  const copyPrimaryLocation = useCallback(async () => {
    const path = primaryFile?.localPath ?? detail?.item.rootPath;
    if (!path) return;
    await copyFilePath(decodeURIComponent(path));
  }, [copyFilePath, detail?.item.rootPath, primaryFile?.localPath]);

  const openPrimaryFile = useCallback(() => {
    if (!primaryFile) return;
    void openOrShareFile(primaryFile);
  }, [openOrShareFile, primaryFile]);

  const openActionsSheet = useCallback(() => {
    actionsSheetRef.current?.present();
  }, []);

  const dismissActionsSheet = useCallback(() => {
    actionsSheetRef.current?.dismiss();
  }, []);

  const runAfterSheetDismiss = useCallback(
    (callback: () => void) => {
      dismissActionsSheet();
      setTimeout(callback, 180);
    },
    [dismissActionsSheet],
  );

  return (
    <AppScreen testID="inbox-detail-screen" contentClassName="px-0 pb-0 pt-0">
      <SettingsHeader
        title={t`收件箱详情`}
        right={detail ? <MoreButton onPress={openActionsSheet} /> : null}
      />

      {detailLoading && !detail ? (
        <View className="flex-1 items-center justify-center gap-3 px-5">
          <ActivityIndicator color={colors.mutedForeground} />
          <Text className="text-[12px] text-muted-foreground">
            <Trans>加载中</Trans>
          </Text>
        </View>
      ) : !detail ? (
        <View className="flex-1 px-5 pt-6">
          <Surface className="items-center gap-3 py-12">
            <FileArchive color={colors.mutedForeground} size={30} />
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>收件箱记录不存在</Trans>
            </Text>
            <Text className="text-center text-[12px] text-muted-foreground">
              <Trans>它可能已经被删除。</Trans>
            </Text>
          </Surface>
        </View>
      ) : (
        <>
          <ScrollView
            className="flex-1"
            showsVerticalScrollIndicator={false}
            contentContainerClassName="gap-5 px-5 pb-8 pt-3"
          >
            <ContentPreview detail={detail} primaryFile={primaryFile} />

            <View className="gap-3" testID="inbox-detail-summary">
              <View className="gap-1.5">
                <Text
                  className="text-[22px] font-bold leading-7 text-foreground"
                  numberOfLines={3}
                >
                  {title}
                </Text>
                <Text className="text-[12px] leading-5 text-muted-foreground">
                  <Trans>来自</Trans> {detail.item.sourceName}
                  {" · "}
                  {formatRelativeTime(detail.item.receivedAt)}
                </Text>
              </View>
              <View className="flex-row flex-wrap gap-2">
                <ContentKindPill
                  kind={detail.item.contentKind}
                  file={primaryFile}
                />
                {detail.item.missing ? (
                  <StatePill tone="missing" label={<Trans>文件缺失</Trans>} />
                ) : null}
                {archived ? (
                  <StatePill tone="muted" label={<Trans>已归档</Trans>} />
                ) : null}
              </View>
              <View className="flex-row gap-2">
                <InfoPill
                  icon={FileArchive}
                  label={<Trans>内容</Trans>}
                  value={
                    <>
                      {fileCount} <Trans>项</Trans>
                    </>
                  }
                />
                <InfoPill
                  icon={HardDrive}
                  label={<Trans>大小</Trans>}
                  value={formatBytes(detail.item.totalSize)}
                />
              </View>
            </View>

            {detail.files.length > 1 ? (
              <IncludedFiles
                files={detail.files}
                onOpenShare={openOrShareFile}
                onCopy={copyFilePath}
              />
            ) : null}

            <DetailsPanel
              detail={detail}
              expanded={detailsExpanded}
              onToggle={() => setDetailsExpanded((value) => !value)}
            />

            {transferSessionId ? (
              <Pressable
                accessibilityRole="button"
                onPress={openTransfer}
                testID="inbox-detail-transfer-link"
                className="min-h-12 flex-row items-center justify-between gap-3 rounded-xl border border-border bg-card px-3.5 py-3 active:opacity-70"
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
          </ScrollView>

          <DetailActionBar
            primaryFile={primaryFile}
            hasLocation={primaryFile != null || detail.item.rootPath != null}
            bottomInset={insets.bottom}
            onOpen={openPrimaryFile}
            onCopy={copyPrimaryLocation}
          />
        </>
      )}

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

      {detail ? (
        <InboxActionsSheet
          sheetRef={actionsSheetRef}
          title={title}
          archived={archived}
          action={action}
          hasTransfer={transferSessionId != null}
          canCopyLocation={primaryFile != null || detail.item.rootPath != null}
          onArchive={() =>
            runAfterSheetDismiss(() => {
              void performArchive();
            })
          }
          onCopyLocation={() =>
            runAfterSheetDismiss(() => {
              void copyPrimaryLocation();
            })
          }
          onOpenTransfer={() => runAfterSheetDismiss(openTransfer)}
          onDeleteRecord={() =>
            runAfterSheetDismiss(() => setDeleteMode("record"))
          }
          onDeleteContent={() =>
            runAfterSheetDismiss(() => setDeleteMode("content"))
          }
        />
      ) : null}
    </AppScreen>
  );
}

function MoreButton({ onPress }: { onPress: () => void }) {
  const colors = useThemeColors();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel="更多"
      onPress={onPress}
      testID="inbox-detail-more-button"
      className="size-11 items-center justify-center rounded-xl bg-muted active:opacity-70"
    >
      <MoreHorizontal color={colors.foreground} size={20} />
    </Pressable>
  );
}

function InboxActionsSheet({
  sheetRef,
  title,
  archived,
  action,
  hasTransfer,
  canCopyLocation,
  onArchive,
  onCopyLocation,
  onOpenTransfer,
  onDeleteRecord,
  onDeleteContent,
}: {
  sheetRef: React.RefObject<BottomSheetModal | null>;
  title: string;
  archived: boolean;
  action: string | null;
  hasTransfer: boolean;
  canCopyLocation: boolean;
  onArchive: () => void;
  onCopyLocation: () => void;
  onOpenTransfer: () => void;
  onDeleteRecord: () => void;
  onDeleteContent: () => void;
}) {
  const colors = useThemeColors();
  const renderBackdrop = useCallback(
    (props: BottomSheetBackdropProps) => (
      <BottomSheetBackdrop
        {...props}
        opacity={0.38}
        appearsOnIndex={0}
        disappearsOnIndex={-1}
        pressBehavior="close"
      />
    ),
    [],
  );

  return (
    <BottomSheetModal
      ref={sheetRef}
      enableDynamicSizing
      enablePanDownToClose
      backdropComponent={renderBackdrop}
      backgroundStyle={{ backgroundColor: colors.card }}
      handleIndicatorStyle={{ backgroundColor: colors.border }}
    >
      <BottomSheetView>
        <View className="gap-3 px-5 pb-6 pt-2" testID="inbox-actions-sheet">
          <View className="gap-1 px-1 pb-1">
            <Text className="text-[17px] font-bold text-foreground">
              <Trans>更多操作</Trans>
            </Text>
            <Text
              className="text-[12px] text-muted-foreground"
              numberOfLines={1}
            >
              {title}
            </Text>
          </View>

          <View className="overflow-hidden rounded-2xl border border-border bg-background">
            <SheetActionRow
              icon={archived ? ArchiveRestore : Archive}
              label={archived ? <Trans>取消归档</Trans> : <Trans>归档</Trans>}
              onPress={onArchive}
              disabled={action === "archive"}
              testID="inbox-detail-archive-action"
            />
            <Divider />
            <SheetActionRow
              icon={Copy}
              label={<Trans>复制保存位置</Trans>}
              onPress={onCopyLocation}
              disabled={!canCopyLocation}
              testID="inbox-detail-copy-location-action"
            />
            {hasTransfer ? (
              <>
                <Divider />
                <SheetActionRow
                  icon={ExternalLink}
                  label={<Trans>查看传输诊断</Trans>}
                  onPress={onOpenTransfer}
                />
              </>
            ) : null}
          </View>

          <View className="overflow-hidden rounded-2xl border border-border bg-background">
            <SheetActionRow
              icon={Trash2}
              label={<Trans>仅删除记录</Trans>}
              onPress={onDeleteRecord}
              disabled={action != null}
              destructive
              testID="inbox-detail-delete-record-button"
            />
            <Divider destructive />
            <SheetActionRow
              icon={Trash2}
              label={<Trans>删除记录和本地文件</Trans>}
              onPress={onDeleteContent}
              disabled={action != null}
              destructive
              testID="inbox-detail-delete-content-button"
            />
          </View>
        </View>
      </BottomSheetView>
    </BottomSheetModal>
  );
}

function SheetActionRow({
  icon: Icon,
  label,
  onPress,
  disabled,
  destructive = false,
  testID,
}: {
  icon: LucideIcon;
  label: React.ReactNode;
  onPress: () => void;
  disabled?: boolean;
  destructive?: boolean;
  testID?: string;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      disabled={disabled}
      testID={testID}
      className="min-h-14 flex-row items-center gap-3 px-3.5 py-3 active:bg-muted disabled:opacity-50"
    >
      <View
        className={cn(
          "size-10 items-center justify-center rounded-xl",
          destructive ? "bg-destructive/10" : "bg-muted",
        )}
      >
        <Icon
          color={destructive ? colors.destructive : colors.foreground}
          size={18}
        />
      </View>
      <Text
        className={cn(
          "min-w-0 flex-1 text-[14px] font-semibold",
          destructive ? "text-destructive" : "text-foreground",
        )}
      >
        {label}
      </Text>
    </Pressable>
  );
}

function Divider({ destructive = false }: { destructive?: boolean }) {
  return (
    <View
      className={cn(
        "ml-[62px] h-px",
        destructive ? "bg-destructive/15" : "bg-border",
      )}
    />
  );
}

function ContentPreview({
  detail,
  primaryFile,
}: {
  detail: MobileInboxItemDetail;
  primaryFile: InboxFileEntry | null;
}) {
  const colors = useThemeColors();
  const preview = previewMeta(detail, primaryFile);
  const canPreviewImage =
    primaryFile != null &&
    !primaryFile.missing &&
    primaryFile.localPath.startsWith("file://") &&
    isImageFile(primaryFile.name);

  return (
    <View
      className="overflow-hidden rounded-[28px] border border-border bg-card"
      style={detailStyles.previewFrame}
      testID="inbox-detail-preview"
    >
      {canPreviewImage ? (
        <Image
          source={{ uri: primaryFile.localPath }}
          resizeMode="cover"
          className="h-full w-full"
        />
      ) : (
        <View className="h-full w-full items-center justify-center gap-5 bg-primary/5 px-8">
          <View className="size-24 items-center justify-center rounded-[28px] bg-background">
            <preview.icon color={preview.color(colors)} size={42} />
          </View>
          <View className="items-center gap-1.5">
            <Text className="text-center text-[15px] font-semibold text-foreground">
              {preview.title}
            </Text>
            <Text className="text-center text-[12px] leading-5 text-muted-foreground">
              {preview.description}
            </Text>
          </View>
        </View>
      )}
    </View>
  );
}

function DetailActionBar({
  primaryFile,
  hasLocation,
  bottomInset,
  onOpen,
  onCopy,
}: {
  primaryFile: InboxFileEntry | null;
  hasLocation: boolean;
  bottomInset: number;
  onOpen: () => void;
  onCopy: () => void;
}) {
  const colors = useThemeColors();
  const canOpen = primaryFile != null && !primaryFile.missing;
  return (
    <View
      className="flex-row items-center gap-3 border-t border-border bg-background px-5 pt-3"
      style={{ paddingBottom: Math.max(bottomInset, 12) }}
      testID="inbox-detail-action-bar"
    >
      <Pressable
        accessibilityRole="button"
        onPress={primaryFile ? onOpen : onCopy}
        disabled={primaryFile ? !canOpen : !hasLocation}
        testID={primaryFile ? "inbox-file-share-0" : "inbox-copy-location"}
        className="min-h-12 flex-1 flex-row items-center justify-center gap-2 rounded-2xl bg-primary px-4 active:opacity-80 disabled:opacity-50"
      >
        {primaryFile ? (
          <Share2 color={colors.primaryForeground} size={18} />
        ) : (
          <Copy color={colors.primaryForeground} size={18} />
        )}
        <Text className="text-[14px] font-semibold text-primary-foreground">
          {primaryFile ? <Trans>打开/分享</Trans> : <Trans>复制保存位置</Trans>}
        </Text>
      </Pressable>
      {primaryFile ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="复制路径"
          onPress={onCopy}
          disabled={!hasLocation}
          hitSlop={8}
          testID="inbox-file-copy-0"
          className="size-12 items-center justify-center rounded-2xl border border-border bg-card active:opacity-70 disabled:opacity-50"
        >
          <Copy color={colors.foreground} size={18} />
        </Pressable>
      ) : null}
    </View>
  );
}

function IncludedFiles({
  files,
  onOpenShare,
  onCopy,
}: {
  files: InboxFileEntry[];
  onOpenShare: (file: InboxFileEntry) => void;
  onCopy: (path: string) => void;
}) {
  return (
    <View className="gap-2.5" testID="inbox-detail-files">
      <View className="flex-row items-center justify-between">
        <Text className="text-[15px] font-semibold text-foreground">
          <Trans>包含内容</Trans>
        </Text>
        <Text className="text-[11px] text-muted-foreground">
          {files.length} <Trans>项</Trans>
        </Text>
      </View>
      <View className="overflow-hidden rounded-2xl border border-border bg-card">
        {files.map((file, index) => (
          <FileRow
            key={file.id}
            file={file}
            index={index}
            onOpenShare={onOpenShare}
            onCopy={onCopy}
            separated={index > 0}
          />
        ))}
      </View>
    </View>
  );
}

function FileRow({
  file,
  index,
  onOpenShare,
  onCopy,
  separated = false,
}: {
  file: InboxFileEntry;
  index: number;
  onOpenShare: (file: InboxFileEntry) => void;
  onCopy: (path: string) => void;
  separated?: boolean;
}) {
  const colors = useThemeColors();
  const Icon = fileIcon(file.name);
  return (
    <View
      testID={`inbox-file-share-${index}`}
      className={cn(
        "min-h-16 flex-row items-center gap-3 bg-card p-3",
        separated ? "border-t border-border" : "",
      )}
    >
      <View className="size-11 items-center justify-center rounded-xl bg-muted">
        {file.missing ? (
          <FileWarning color={colors.destructive} size={18} />
        ) : (
          <Icon color={colors.foreground} size={18} />
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
        {file.checksum ? (
          <Text
            className="font-mono text-[10px] text-muted-foreground"
            numberOfLines={1}
          >
            {file.checksum}
          </Text>
        ) : null}
      </View>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="打开或分享"
        onPress={() => onOpenShare(file)}
        disabled={file.missing}
        hitSlop={8}
        className="size-10 items-center justify-center rounded-xl bg-muted active:opacity-70 disabled:opacity-50"
      >
        <ExternalLink color={colors.foreground} size={16} />
      </Pressable>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="复制路径"
        onPress={(event) => {
          event.stopPropagation();
          onCopy(file.localPath);
        }}
        hitSlop={8}
        testID={`inbox-file-copy-${index}`}
        className="size-10 items-center justify-center rounded-xl bg-muted active:opacity-70"
      >
        <Copy color={colors.foreground} size={16} />
      </Pressable>
    </View>
  );
}

function DetailsPanel({
  detail,
  expanded,
  onToggle,
}: {
  detail: MobileInboxItemDetail;
  expanded: boolean;
  onToggle: () => void;
}) {
  const colors = useThemeColors();
  return (
    <View className="overflow-hidden rounded-2xl border border-border bg-card">
      <Pressable
        accessibilityRole="button"
        onPress={onToggle}
        className="min-h-12 flex-row items-center justify-between px-3.5 py-3 active:opacity-70"
      >
        <View className="flex-row items-center gap-2">
          <View className="size-8 items-center justify-center rounded-xl bg-muted">
            <Database color={colors.foreground} size={15} />
          </View>
          <Text className="text-[14px] font-semibold text-foreground">
            <Trans>详情</Trans>
          </Text>
        </View>
        {expanded ? (
          <ChevronUp color={colors.mutedForeground} size={17} />
        ) : (
          <ChevronDown color={colors.mutedForeground} size={17} />
        )}
      </Pressable>
      <View className="gap-3 border-t border-border px-3.5 py-3">
        <DetailLine
          icon={CalendarClock}
          label={<Trans>接收时间</Trans>}
          value={new Date(Number(detail.item.receivedAt)).toLocaleString()}
        />
        <DetailLine
          icon={FolderOpen}
          label={<Trans>保存位置</Trans>}
          value={
            detail.item.rootPath ? (
              decodeURIComponent(detail.item.rootPath)
            ) : (
              <Trans>未记录</Trans>
            )
          }
          mono
        />
        {expanded ? (
          <>
            <DetailLine
              icon={Tag}
              label={<Trans>来源类型</Trans>}
              value={<SourceKindLabel kind={detail.item.sourceKind} />}
            />
            <DetailLine
              icon={Smartphone}
              label={<Trans>来源设备</Trans>}
              value={detail.item.sourcePeerId}
              mono
            />
            {detail.item.lastOpenedAt != null ? (
              <DetailLine
                icon={ExternalLink}
                label={<Trans>上次打开</Trans>}
                value={new Date(
                  Number(detail.item.lastOpenedAt),
                ).toLocaleString()}
              />
            ) : null}
            {detail.item.contentHash ? (
              <DetailLine
                icon={Database}
                label={<Trans>内容指纹</Trans>}
                value={detail.item.contentHash}
                mono
              />
            ) : null}
          </>
        ) : null}
      </View>
    </View>
  );
}

function ContentKindPill({
  kind,
  file,
}: {
  kind: MobileInboxContentKind;
  file?: InboxFileEntry | null;
}) {
  const label = file ? (
    contentLabel(file.name, kind)
  ) : kind === MobileInboxContentKind.Files ? (
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

/** 收件箱来源类型:已配对设备 / 配对码 / AI 代理(MCP) / 未知,镜像桌面 sourceKindLabel。 */
function SourceKindLabel({ kind }: { kind: MobileInboxSourceKind }) {
  switch (kind) {
    case MobileInboxSourceKind.PairedDevice:
      return <Trans>已配对设备</Trans>;
    case MobileInboxSourceKind.ShareCode:
      return <Trans>配对码</Trans>;
    case MobileInboxSourceKind.Mcp:
      return <Trans>AI 代理 (MCP)</Trans>;
    default:
      return <Trans>未知</Trans>;
  }
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

function InfoPill({
  icon: Icon,
  label,
  value,
}: {
  icon: LucideIcon;
  label: React.ReactNode;
  value: React.ReactNode;
}) {
  const colors = useThemeColors();
  return (
    <View className="min-h-14 flex-1 flex-row items-center gap-2 rounded-2xl bg-primary/5 px-3">
      <Icon color={colors.primary} size={15} />
      <View className="min-w-0 flex-1">
        <Text className="text-[10px] text-muted-foreground">{label}</Text>
        <Text className="text-[12px] font-semibold text-foreground">
          {value}
        </Text>
      </View>
    </View>
  );
}

function DetailLine({
  icon: Icon,
  label,
  value,
  mono = false,
}: {
  icon: LucideIcon;
  label: React.ReactNode;
  value: React.ReactNode;
  mono?: boolean;
}) {
  const colors = useThemeColors();
  return (
    <View className="flex-row items-start gap-2">
      <Icon color={colors.mutedForeground} size={14} />
      <View className="min-w-0 flex-1">
        <Text className="text-[11px] text-muted-foreground">{label}</Text>
        <Text
          className={cn(
            "mt-0.5 text-[12px] text-foreground",
            mono ? "font-mono" : "",
          )}
          numberOfLines={3}
        >
          {value}
        </Text>
      </View>
    </View>
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

function isMissingFileError(err: unknown, file: InboxFileEntry): boolean {
  if (err instanceof MissingFileError) return true;
  // 不靠错误文案判断（本地化 / 不同平台下英文子串会漏判）：直接复查文件是否还在原位。
  if (!file.localPath.startsWith("file://")) return false;
  try {
    return !new File(file.localPath).exists;
  } catch {
    return false;
  }
}

function previewMeta(
  detail: MobileInboxItemDetail,
  primaryFile: InboxFileEntry | null,
): {
  icon: LucideIcon;
  title: React.ReactNode;
  description: React.ReactNode;
  color: (colors: ReturnType<typeof useThemeColors>) => string;
} {
  if (detail.item.missing || primaryFile?.missing) {
    return {
      icon: FileWarning,
      title: <Trans>文件已不在原位置</Trans>,
      description: <Trans>可以保留记录，或从更多菜单清理这条记录。</Trans>,
      color: (colors) => colors.destructive,
    };
  }
  if (detail.files.length > 1) {
    return {
      icon: Package,
      title: (
        <>
          {detail.files.length} <Trans>项内容</Trans>
        </>
      ),
      description: (
        <Trans>这是一组收到的内容，下方可以逐项打开或复制路径。</Trans>
      ),
      color: (colors) => colors.primary,
    };
  }
  if (primaryFile && isVideoFile(primaryFile.name)) {
    return {
      icon: Video,
      title: extensionName(primaryFile.name),
      description: <Trans>视频文件，可以通过系统分享面板打开。</Trans>,
      color: (colors) => colors.primary,
    };
  }
  if (primaryFile && isImageFile(primaryFile.name)) {
    return {
      icon: ImageIcon,
      title: extensionName(primaryFile.name),
      description: <Trans>图片文件</Trans>,
      color: (colors) => colors.primary,
    };
  }
  if (detail.item.contentKind === MobileInboxContentKind.Clipboard) {
    return {
      icon: ClipboardList,
      title: <Trans>剪贴板内容</Trans>,
      description: <Trans>从另一台设备接收的剪贴板记录。</Trans>,
      color: (colors) => colors.primary,
    };
  }
  if (detail.item.contentKind === MobileInboxContentKind.Text) {
    return {
      icon: FileText,
      title: <Trans>文本内容</Trans>,
      description: <Trans>从另一台设备接收的文本记录。</Trans>,
      color: (colors) => colors.primary,
    };
  }
  return {
    icon: primaryFile ? fileIcon(primaryFile.name) : FileArchive,
    title: primaryFile ? (
      extensionName(primaryFile.name)
    ) : (
      <Trans>收到的内容</Trans>
    ),
    description: primaryFile ? (
      <Trans>可以打开、分享或复制本地保存路径。</Trans>
    ) : (
      <Trans>这条记录暂时没有可打开的本地文件。</Trans>
    ),
    color: (colors) => colors.primary,
  };
}

function fileIcon(name: string): LucideIcon {
  if (isImageFile(name)) return ImageIcon;
  if (isVideoFile(name)) return Video;
  return FileArchive;
}

function contentLabel(name: string, kind: MobileInboxContentKind) {
  if (isImageFile(name)) return <Trans>图片</Trans>;
  if (isVideoFile(name)) return <Trans>视频</Trans>;
  if (kind === MobileInboxContentKind.Text) return <Trans>文本</Trans>;
  if (kind === MobileInboxContentKind.Clipboard) return <Trans>剪贴板</Trans>;
  if (kind === MobileInboxContentKind.Bundle) return <Trans>组合内容</Trans>;
  return <Trans>文件</Trans>;
}

function extensionName(name: string): string {
  const match = /\.([^.]+)$/.exec(name);
  return match ? match[1].toUpperCase() : "FILE";
}

function isImageFile(name: string): boolean {
  return hasAnyExtension(name, IMAGE_EXTENSIONS);
}

function isVideoFile(name: string): boolean {
  return hasAnyExtension(name, VIDEO_EXTENSIONS);
}

function hasAnyExtension(name: string, extensions: string[]): boolean {
  const lower = name.toLowerCase();
  return extensions.some((extension) => lower.endsWith(extension));
}

const IMAGE_EXTENSIONS = [
  ".jpg",
  ".jpeg",
  ".png",
  ".gif",
  ".webp",
  ".heic",
  ".heif",
  ".bmp",
  ".tiff",
  ".avif",
];

const VIDEO_EXTENSIONS = [
  ".mp4",
  ".mov",
  ".m4v",
  ".webm",
  ".mkv",
  ".avi",
  ".wmv",
  ".flv",
  ".3gp",
];

const detailStyles = StyleSheet.create({
  previewFrame: {
    aspectRatio: 1.08,
  },
});
