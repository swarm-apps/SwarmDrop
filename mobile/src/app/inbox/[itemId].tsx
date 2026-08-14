import { Trans, useLingui } from "@lingui/react/macro";
import { File } from "expo-file-system";
import {
  useFocusEffect,
  useLocalSearchParams,
  useNavigation,
  useRouter,
} from "expo-router";
import { useVideoPlayer, type VideoPlayer, VideoView } from "expo-video";
import {
  Archive,
  ArchiveRestore,
  CalendarClock,
  ChevronDown,
  ChevronUp,
  ClipboardList,
  Database,
  ExternalLink,
  Eye,
  FileText,
  FileWarning,
  FolderOpen,
  Inbox,
  type LucideIcon,
  MoreHorizontal,
  Package,
  Send,
  Share2,
  Smartphone,
  Tag,
  Trash2,
  TriangleAlert,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Image, Pressable, ScrollView, StyleSheet, View } from "react-native";
import ImageViewing from "react-native-image-viewing";
import {
  MobileInboxContentKind,
  MobileInboxItemContent,
  type MobileInboxItemDetail,
  MobileInboxSourceKind,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  FileBrowser,
  type FileBrowserActions,
  fileBrowserIcon,
  fromInboxFiles,
  inboxFileId,
  isImageFile,
  isVideoFile,
} from "@/components/file-browser";
import {
  AppScreen,
  BottomActionBar,
  Surface,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { formatBytes, formatRelativeTime } from "@/components/transfer/shared";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Text } from "@/components/ui/text";
import {
  ensureAvailable,
  isMissingFileError,
  selectForwardable,
} from "@/core/inbox-file-availability";
import { canOpenSaveFolder } from "@/core/saf-intent";
import { useThemeColors } from "@/hooks/useThemeColors";
import { clipboard } from "@/lib/clipboard";
import { inboxItemTitle } from "@/lib/inbox-title";
import { openFileWithSystem, shareFileWithSystem } from "@/lib/open-file";
import { openSaveFolderOrToast } from "@/lib/save-folder";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { type InboxFileEntry, useInboxStore } from "@/stores/inbox-store";
import { useShareStore } from "@/stores/share-store";

export default function InboxDetailScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const actionsSheetRef = useRef<AppBottomSheetRef>(null);
  const { itemId } = useLocalSearchParams<{ itemId: string }>();
  const [deleteMode, setDeleteMode] = useState<"record" | "content" | null>(
    null,
  );
  const [detailsExpanded, setDetailsExpanded] = useState(false);
  const {
    detail,
    detailLoading,
    action,
    lastError,
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
      lastError: s.lastError,
      loadDetail: s.loadDetail,
      clearDetail: s.clearDetail,
      markOpened: s.markOpened,
      archiveItem: s.archiveItem,
      deleteItem: s.deleteItem,
      markFileMissing: s.markFileMissing,
    })),
  );

  const setSharedFiles = useShareStore((s) => s.setSharedFiles);

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

  // 加载失败(抛异常,如网络/DB 瞬时问题)与"合法 not-found"(getInboxItem 正常返回
  // undefined)在 store 里已经可区分——前者会带 lastError,后者不会。重试只重新调用同一
  // 次 loadDetail,不做额外假设。
  const retryLoadDetail = useCallback(() => {
    if (!itemId) return;
    void (async () => {
      const loaded = await loadDetail(itemId);
      if (loaded) {
        void markOpened(itemId);
      }
    })();
  }, [itemId, loadDetail, markOpened]);

  // 仅在真正离开详情页（卸载）时清空，而非失焦——否则跳转「传输诊断」再返回会因
  // selectedDetail 被清空而触发整屏「加载中」reload 闪烁。重新聚焦会刷新数据，但旧详情
  // 仍在，不会闪烁。
  useEffect(() => {
    return () => clearDetail();
  }, [clearDetail]);

  const archived = detail?.item.archivedAt != null;
  const title = detail
    ? inboxItemTitle(detail.item.title, detail.item.itemCount)
    : t`收件箱详情`;
  const files =
    detail && MobileInboxItemContent.Files.instanceOf(detail.content)
      ? detail.content.inner.entries
      : [];
  const textBody =
    detail && MobileInboxItemContent.Text.instanceOf(detail.content)
      ? detail.content.inner.body
      : null;
  const fileCount = files.length;
  const primaryFile = files.length === 1 ? files[0] : null;
  const itemMissing =
    detail?.item.missing === true || primaryFile?.missing === true;
  const contentKind = detail?.item.contentKind;
  // 只有"有真内容可看"(图片/文本正文)才配大预览;其余形态(通用文件/多文件/缺失)
  // 用标题区的类型图标 chip 承载识别,不再渲染大而空的占位框。
  // 做成"可空文件引用"而非 boolean:JSX 分支直接窄化,不用二次判空。
  const previewImageFile =
    primaryFile != null &&
    !primaryFile.missing &&
    primaryFile.localPath.startsWith("file://") &&
    isImageFile(primaryFile.name)
      ? primaryFile
      : null;
  // 视频与图片镜像判定(file:// only,SAF content:// 走「打开」交系统播放器,见 design R5)
  const previewVideoFile =
    primaryFile != null &&
    !primaryFile.missing &&
    primaryFile.localPath.startsWith("file://") &&
    isVideoFile(primaryFile.name)
      ? primaryFile
      : null;
  const excerptFile =
    !itemMissing &&
    previewImageFile == null &&
    previewVideoFile == null &&
    (contentKind === MobileInboxContentKind.Text ||
      contentKind === MobileInboxContentKind.Clipboard)
      ? primaryFile
      : null;
  const hasRichPreview =
    previewImageFile != null || previewVideoFile != null || excerptFile != null;

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

  const shareFile = useCallback(
    async (file: InboxFileEntry) => {
      if (!itemId) return;
      try {
        await ensureAvailable(file);
        await shareFileWithSystem(file.localPath, file.name, t`分享文件`);
      } catch (err) {
        if (isMissingFileError(err, file)) {
          await markFileMissing(itemId, file.id, true);
          toast.error(t`文件已不在原位置`, file.localPath);
          return;
        }
        toast.error(t`分享失败`, err);
      }
    },
    [itemId, markFileMissing, t],
  );

  /**
   * 转发到另一台设备 —— 复用「文件已定 → 挑设备」这条既有反向流（系统分享入口与
   * 「重新发送」走的也是它），不新增传输概念。
   *
   * 在**发起前**筛掉已不在原位的文件：`prepareSend` 会为每个文件算 BLAKE3，一个死 URI
   * 会让整批准备失败，而用户看到的错误与「哪个文件没了」毫无关系。
   */
  const forwardFiles = useCallback(
    async (entries: readonly InboxFileEntry[]) => {
      if (!itemId || entries.length === 0) return;
      const { files, missing } = selectForwardable(entries);
      // `allSettled` 而非串行 await：这些写彼此独立（store 用函数式 set，并发安全），
      // 而串行版每次都会替换 `selectedDetail`、把整个文件列表重画一遍。
      //
      // 更要紧的是**不能让标记失败挡住转发**：`markFileMissing` 会 rethrow，串行 await
      // 的第一次失败就会抛出整个 `forwardFiles`，而调用点是 `void forwardFiles(...)`
      // ——用户看不到任何提示，转发也没发生。文件确实不在了这件事已经判定完毕，记录没
      // 更新上不该改变结论。
      await Promise.allSettled(
        missing.map((entry) => markFileMissing(itemId, entry.id, true)),
      );
      if (files.length === 0) {
        toast.error(t`文件已不在原位置`);
        return;
      }
      if (missing.length > 0) {
        toast.info(t`已跳过 ${missing.length} 个不在原位置的文件`);
      }
      setSharedFiles(files);
      router.push("/send/share-target" as never);
    },
    [itemId, markFileMissing, setSharedFiles, router, t],
  );

  // 「打开」= 让用户看到内容:iOS QuickLook / Android 系统应用。
  // 打不开(无处理应用)降级到分享面板 —— 至少能把文件带去别的应用,
  // 这也是分享路径仍然保留的原因(design R4)。
  const openFile = useCallback(
    async (file: InboxFileEntry) => {
      if (!itemId) return;
      try {
        await ensureAvailable(file);
        try {
          await openFileWithSystem(file.localPath);
          return;
        } catch (openErr) {
          if (isMissingFileError(openErr, file)) throw openErr;
          await shareFileWithSystem(file.localPath, file.name, t`分享文件`);
        }
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

  // 打开文件夹:直接用记录的真实容器目录 rootPath(core 以 finalize_sink 的文件父目录
  // URI 为事实源算出)。canOpenSaveFolder=false 时入口不渲染 —— 落点治本后这只剩
  // 「历史记录里的旧形态 URI」一种情形，当前配置产出的落点两端都恒可打开。
  const folderTarget = detail?.item.rootPath ?? null;
  const canOpenFolder = folderTarget != null && canOpenSaveFolder(folderTarget);
  const openFolder = useCallback(() => {
    if (!folderTarget) return;
    void openSaveFolderOrToast(folderTarget);
  }, [folderTarget]);

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

  const openPrimaryFile = useCallback(() => {
    if (!primaryFile) return;
    void openFile(primaryFile);
  }, [openFile, primaryFile]);

  const canShare = primaryFile != null && !primaryFile.missing;
  // 整条记录级的转发：只要还有一个文件没被标记缺失就给入口，具体哪些能发由
  // `selectForwardable` 在发起时逐个探活决定。
  const canSend = files.some((file) => !file.missing);
  const sharePrimaryFile = useCallback(() => {
    if (!primaryFile) return;
    void shareFile(primaryFile);
  }, [shareFile, primaryFile]);

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

  const fileItems = useMemo(
    () => (detail && itemId ? fromInboxFiles(itemId, files) : []),
    [detail, files, itemId],
  );
  const fileBrowserActions = useMemo<FileBrowserActions>(
    () => ({
      openItem: (item) => {
        const file = files.find(
          (candidate) =>
            itemId && inboxFileId(itemId, candidate.id) === item.id,
        );
        if (file) void openFile(file);
      },
      shareItem: (item) => {
        const file = files.find(
          (candidate) =>
            itemId && inboxFileId(itemId, candidate.id) === item.id,
        );
        if (file) void shareFile(file);
      },
      sendItem: (item) => {
        const file = files.find(
          (candidate) =>
            itemId && inboxFileId(itemId, candidate.id) === item.id,
        );
        if (file) void forwardFiles([file]);
      },
    }),
    [files, itemId, openFile, shareFile, forwardFiles],
  );

  return (
    <AppScreen
      testID="inbox-detail-screen"
      header={
        <SettingsHeader
          title={t`收件箱详情`}
          right={detail ? <MoreButton onPress={openActionsSheet} /> : null}
        />
      }
      bare
    >
      {detailLoading && !detail ? (
        // 骨架屏镜像正常分支布局:类型 chip + 标题块 → 详情卡行
        <View
          accessible
          accessibilityLabel={t`加载中`}
          className="flex-1 gap-5 px-5 pt-3"
        >
          <View className="flex-row items-center gap-3">
            <Skeleton className="size-14 rounded-xl" />
            <View className="min-w-0 flex-1 gap-1.5">
              <Skeleton className="h-5 w-2/3" />
              <Skeleton className="h-3 w-1/2" />
            </View>
          </View>
          <View className="overflow-hidden rounded-lg border border-border bg-card">
            {[0, 1].map((row) => (
              <View
                key={row}
                className={cn(
                  "min-h-16 flex-row items-center gap-3 p-3",
                  row > 0 ? "border-t border-border" : "",
                )}
              >
                <Skeleton className="size-11 rounded-xl" />
                <View className="min-w-0 flex-1 gap-1.5">
                  <Skeleton className="h-3.5 w-1/2" />
                  <Skeleton className="h-3 w-1/3" />
                </View>
              </View>
            ))}
          </View>
        </View>
      ) : !detail && lastError ? (
        // 加载调用抛出了异常(瞬时问题),不是后端确认的"记录不存在"——不能断言"可能已删除",
        // 只能如实说明加载失败,并给出重试入口。
        <View className="flex-1 px-5 pt-6">
          <Surface
            className="items-center gap-3 py-12"
            testID="inbox-detail-error-state"
          >
            <FileWarning color={colors.destructive} size={30} />
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>暂时无法打开，请重试</Trans>
            </Text>
            <Text className="text-center text-[13px] text-muted-foreground">
              {lastError}
            </Text>
            <Pressable
              accessibilityRole="button"
              onPress={retryLoadDetail}
              testID="inbox-detail-retry-button"
              className="min-h-11 min-w-24 items-center justify-center rounded-xl bg-primary px-4 active:opacity-70"
            >
              <Text className="text-[13px] font-semibold text-primary-foreground">
                <Trans>重试</Trans>
              </Text>
            </Pressable>
          </Surface>
        </View>
      ) : !detail ? (
        // getInboxItem 正常返回了 undefined——后端已确认这条记录真的不存在,不提供重试。
        <View className="flex-1 px-5 pt-6">
          <Surface
            className="items-center gap-3 py-12"
            testID="inbox-detail-missing-state"
          >
            <TriangleAlert color={colors.mutedForeground} size={30} />
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>收件箱记录不存在</Trans>
            </Text>
            <Text className="text-center text-[13px] text-muted-foreground">
              <Trans>它可能已经被删除。</Trans>
            </Text>
          </Surface>
        </View>
      ) : (
        <>
          {files.length > 1 ? (
            <FileBrowser
              items={fileItems}
              scope="inbox"
              actions={fileBrowserActions}
              resetKey={detail.item.id}
              testID="inbox-detail-file-browser"
              title={<Trans>包含内容</Trans>}
              contentHeader={
                <InboxDetailContent
                  detail={detail}
                  title={title}
                  archived={archived}
                  itemMissing={itemMissing}
                  fileCount={fileCount}
                  primaryFile={primaryFile}
                  hasRichPreview={hasRichPreview}
                  previewImageFile={previewImageFile}
                  previewVideoFile={previewVideoFile}
                  excerptFile={excerptFile}
                  textBody={textBody}
                  detailsExpanded={detailsExpanded}
                  onToggleDetails={() => setDetailsExpanded((value) => !value)}
                  transferSessionId={transferSessionId}
                  onOpenTransfer={openTransfer}
                />
              }
            />
          ) : (
            <ScrollView
              className="flex-1"
              showsVerticalScrollIndicator={false}
              contentContainerClassName="px-5 pb-8 pt-3"
            >
              <InboxDetailContent
                detail={detail}
                title={title}
                archived={archived}
                itemMissing={itemMissing}
                fileCount={fileCount}
                primaryFile={primaryFile}
                hasRichPreview={hasRichPreview}
                previewImageFile={previewImageFile}
                previewVideoFile={previewVideoFile}
                excerptFile={excerptFile}
                textBody={textBody}
                detailsExpanded={detailsExpanded}
                onToggleDetails={() => setDetailsExpanded((value) => !value)}
                transferSessionId={transferSessionId}
                onOpenTransfer={openTransfer}
              />
            </ScrollView>
          )}

          <DetailActionBar
            primaryFile={primaryFile}
            hasFolder={canOpenFolder}
            onOpen={openPrimaryFile}
            onOpenFolder={openFolder}
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
          canOpenFolder={canOpenFolder}
          canShare={canShare}
          onShare={() =>
            runAfterSheetDismiss(() => {
              sharePrimaryFile();
            })
          }
          canSend={canSend}
          onSend={() =>
            runAfterSheetDismiss(() => {
              void forwardFiles(files);
            })
          }
          onArchive={() =>
            runAfterSheetDismiss(() => {
              void performArchive();
            })
          }
          onOpenFolder={() =>
            runAfterSheetDismiss(() => {
              void openFolder();
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

interface InboxDetailContentProps {
  detail: MobileInboxItemDetail;
  title: string;
  archived: boolean;
  itemMissing: boolean;
  fileCount: number;
  primaryFile: InboxFileEntry | null;
  hasRichPreview: boolean;
  previewImageFile: InboxFileEntry | null;
  previewVideoFile: InboxFileEntry | null;
  excerptFile: InboxFileEntry | null;
  textBody: string | null;
  detailsExpanded: boolean;
  onToggleDetails: () => void;
  transferSessionId?: string;
  onOpenTransfer: () => void;
}

function InboxDetailContent({
  detail,
  title,
  archived,
  itemMissing,
  fileCount,
  primaryFile,
  hasRichPreview,
  previewImageFile,
  previewVideoFile,
  excerptFile,
  textBody,
  detailsExpanded,
  onToggleDetails,
  transferSessionId,
  onOpenTransfer,
}: InboxDetailContentProps) {
  const colors = useThemeColors();

  return (
    <View className="gap-5">
      {previewImageFile ? (
        <ImagePreview file={previewImageFile} />
      ) : previewVideoFile ? (
        <VideoPreview file={previewVideoFile} />
      ) : textBody != null ? (
        <TextDeliveryCard body={textBody} />
      ) : excerptFile ? (
        <TextExcerptCard kind={detail.item.contentKind} file={excerptFile} />
      ) : null}

      <View className="gap-3" testID="inbox-detail-summary">
        <View className="flex-row items-center gap-3">
          {!hasRichPreview ? (
            <TypeChip
              missing={itemMissing}
              multi={fileCount > 1}
              primaryFile={primaryFile}
            />
          ) : null}
          <View className="min-w-0 flex-1 gap-1">
            <Text
              className="text-[20px] font-semibold leading-7 tracking-tight text-foreground"
              numberOfLines={3}
            >
              {title}
            </Text>
            <Text
              className="text-[13px] leading-5 text-muted-foreground"
              numberOfLines={1}
            >
              <Trans>来自</Trans> {detail.item.sourceName}
              {" · "}
              {formatRelativeTime(detail.item.receivedAt)}
              {" · "}
              {fileCount > 1 ? (
                <>
                  {fileCount} <Trans>项</Trans>
                  {" · "}
                </>
              ) : null}
              {formatBytes(detail.item.totalSize)}
            </Text>
          </View>
        </View>
        {detail.item.missing || archived ? (
          <View className="flex-row flex-wrap gap-2">
            {detail.item.missing ? (
              <StatePill tone="missing" label={<Trans>文件缺失</Trans>} />
            ) : null}
            {archived ? (
              <StatePill tone="muted" label={<Trans>已归档</Trans>} />
            ) : null}
          </View>
        ) : null}
      </View>

      <DetailsPanel
        detail={detail}
        expanded={detailsExpanded}
        onToggle={onToggleDetails}
      />

      {transferSessionId ? (
        <Pressable
          accessibilityRole="button"
          onPress={onOpenTransfer}
          testID="inbox-detail-transfer-link"
          className="min-h-12 flex-row items-center justify-between gap-3 rounded-lg border border-border bg-card px-3.5 active:opacity-70"
        >
          <Text className="text-[13px] font-semibold text-foreground">
            <Trans>查看传输过程</Trans>
          </Text>
          <ExternalLink color={colors.mutedForeground} size={17} />
        </Pressable>
      ) : null}
    </View>
  );
}

function MoreButton({ onPress }: { onPress: () => void }) {
  const { t } = useLingui();
  const colors = useThemeColors();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={t`更多`}
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
  canOpenFolder,
  canShare,
  onShare,
  canSend,
  onSend,
  onArchive,
  onOpenFolder,
  onOpenTransfer,
  onDeleteRecord,
  onDeleteContent,
}: {
  sheetRef: React.RefObject<AppBottomSheetRef | null>;
  title: string;
  archived: boolean;
  action: string | null;
  hasTransfer: boolean;
  canOpenFolder: boolean;
  canShare: boolean;
  onShare: () => void;
  canSend: boolean;
  onSend: () => void;
  onArchive: () => void;
  onOpenFolder: () => void;
  onOpenTransfer: () => void;
  onDeleteRecord: () => void;
  onDeleteContent: () => void;
}) {
  return (
    <AppBottomSheet ref={sheetRef}>
      <View className="gap-3 px-5 pb-6 pt-2" testID="inbox-actions-sheet">
        <View className="gap-1 px-1 pb-1">
          <Text className="text-[17px] font-bold text-foreground">
            <Trans>更多操作</Trans>
          </Text>
          <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
            {title}
          </Text>
        </View>

        <View className="overflow-hidden rounded-lg border border-border bg-background">
          {canSend ? (
            <>
              <SheetActionRow
                icon={Send}
                label={<Trans>发送到设备</Trans>}
                onPress={onSend}
                testID="inbox-detail-send-action"
              />
              <Divider />
            </>
          ) : null}
          {canShare ? (
            <>
              <SheetActionRow
                icon={Share2}
                label={<Trans>分享</Trans>}
                onPress={onShare}
                testID="inbox-detail-share-action"
              />
              <Divider />
            </>
          ) : null}
          <SheetActionRow
            icon={archived ? ArchiveRestore : Archive}
            label={archived ? <Trans>取消归档</Trans> : <Trans>归档</Trans>}
            onPress={onArchive}
            disabled={action === "archive"}
            testID="inbox-detail-archive-action"
          />
          {canOpenFolder ? (
            <>
              <Divider />
              <SheetActionRow
                icon={FolderOpen}
                label={<Trans>打开文件夹</Trans>}
                onPress={onOpenFolder}
                testID="inbox-detail-open-folder-action"
              />
            </>
          ) : null}
          {hasTransfer ? (
            <>
              <Divider />
              <SheetActionRow
                icon={ExternalLink}
                label={<Trans>查看传输过程</Trans>}
                onPress={onOpenTransfer}
              />
            </>
          ) : null}
        </View>

        <View className="overflow-hidden rounded-lg border border-border bg-background">
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
    </AppBottomSheet>
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
          destructive ? "text-destructive-ink" : "text-foreground",
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

/** 文本/剪贴板正文摘录的最大展示字符数(超出截断并加省略号)。 */
const TEXT_EXCERPT_MAX_CHARS = 400;

function truncateExcerpt(text: string): string {
  const collapsed = text.trim();
  if (collapsed.length <= TEXT_EXCERPT_MAX_CHARS) return collapsed;
  return `${collapsed.slice(0, TEXT_EXCERPT_MAX_CHARS)}…`;
}

/** 图片:真的有内容可看,大面积才花得值 —— 点击进应用内全屏查看(缩放/手势关闭)。 */
function ImagePreview({ file }: { file: InboxFileEntry }) {
  const { t } = useLingui();
  const [viewerVisible, setViewerVisible] = useState(false);
  return (
    <>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={t`全屏查看 ${file.name}`}
        onPress={() => setViewerVisible(true)}
        className="overflow-hidden rounded-lg border border-border bg-card active:opacity-90"
        style={detailStyles.previewFrame}
        testID="inbox-detail-preview"
      >
        <Image
          source={{ uri: file.localPath }}
          resizeMode="cover"
          className="h-full w-full"
          accessibilityLabel={file.name}
        />
      </Pressable>
      <ImageViewing
        images={[{ uri: file.localPath }]}
        imageIndex={0}
        visible={viewerVisible}
        onRequestClose={() => setViewerVisible(false)}
      />
    </>
  );
}

/** 视频:同图片占大预览位,内联原生控制条,不自动播放(spec: Inline video playback)。 */
function VideoPreview({ file }: { file: InboxFileEntry }) {
  const navigation = useNavigation();

  // ⚠️ 下面这个 effect **必须声明在 useVideoPlayer 之前**,这是整段的要害:
  // cleanup 按 hook 声明顺序执行,声明在前 → destroy 先跑 → pause 打在**还活着**的 player 上。
  // 顺序反过来(此前用 useFocusEffect 就是)则 useVideoPlayer 的 release 先执行,pause 撞上
  // 已释放的 SharedObject,抛 NativeSharedObjectNotFoundException —— 补 ErrorBoundary 之前
  // 那就是整个 App 闪退,症状是「打开带视频的收件箱条目,返回即崩」。
  //
  // 用 ref 间接持有 player 是这个顺序的代价:声明在前就没法直接闭包引用它(TDZ)。
  // 换来的是两条路径都真实生效、且**不需要任何 try/catch**:
  //   ① 路由失焦(app 内跳走,本屏仍挂载)—— 不暂停音频会跟到别的页面;
  //   ② 卸载(返回,或 detail 刷新后不再渲染本组件)。
  // ② 尤其不能省:expo-video 56 靠 release() 顺带停播,而 SDK 57 把这条性质回归掉了
  // (native 对象活到 JS GC,expo/expo#47569),届时只剩这次 pause 能停住。
  // 详见 knowledge/toolchain.md 的「expo-video 的停播依赖」。
  const playerRef = useRef<VideoPlayer | null>(null);
  useEffect(() => {
    const pause = () => playerRef.current?.pause();
    const unsubscribe = navigation.addListener("blur", pause);
    return () => {
      unsubscribe();
      pause();
    };
  }, [navigation]);

  const player = useVideoPlayer(file.localPath);
  useEffect(() => {
    playerRef.current = player;
  }, [player]);

  return (
    <View
      className="overflow-hidden rounded-lg border border-border bg-card"
      style={detailStyles.previewFrame}
      testID="inbox-detail-video-preview"
    >
      <VideoView
        player={player}
        style={detailStyles.videoSurface}
        contentFit="contain"
        nativeControls
        accessibilityLabel={file.name}
      />
    </View>
  );
}

/** 文本/剪贴板:内联展示真实收到的正文,而不是纯装饰性图标留白。 */
function TextExcerptCard({
  kind,
  file,
}: {
  kind: MobileInboxContentKind;
  file: InboxFileEntry;
}) {
  const colors = useThemeColors();
  const [textExcerpt, setTextExcerpt] = useState<string | null>(null);
  const [textReadFailed, setTextReadFailed] = useState(false);

  // 依赖原始值而非 file 对象:refocus 会整体替换 detail(全新对象),按引用依赖
  // 每次返回本页都会闪「加载中」并重读整个文件。
  const { localPath, missing } = file;
  useEffect(() => {
    setTextExcerpt(null);
    setTextReadFailed(false);
    if (missing || !localPath.startsWith("file://")) {
      setTextReadFailed(true);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const localFile = new File(localPath);
        if (!localFile.exists) {
          if (!cancelled) setTextReadFailed(true);
          return;
        }
        const content = await localFile.text();
        // 写入前截断:state 只留展示所需的 ~400 字符,MB 级文本不常驻内存
        if (!cancelled) setTextExcerpt(truncateExcerpt(content));
      } catch {
        if (!cancelled) setTextReadFailed(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [localPath, missing]);

  const isClipboard = kind === MobileInboxContentKind.Clipboard;
  const Icon = isClipboard ? ClipboardList : FileText;
  return (
    <Surface className="gap-2.5 px-4 py-4" testID="inbox-detail-text-excerpt">
      <View className="flex-row items-center gap-2">
        <Icon color={colors.mutedForeground} size={15} />
        <Text className="text-[13px] font-medium text-muted-foreground">
          {isClipboard ? <Trans>剪贴板内容</Trans> : <Trans>文本内容</Trans>}
        </Text>
      </View>
      <Text className="text-[13px] leading-5 text-foreground" numberOfLines={8}>
        {textExcerpt != null ? (
          textExcerpt
        ) : textReadFailed ? (
          <Trans>暂时无法读取正文内容。</Trans>
        ) : (
          <Trans>正在加载内容…</Trans>
        )}
      </Text>
    </Surface>
  );
}

/** 文本投递正文已随收件箱详情返回，复制只能由用户显式触发。 */
function TextDeliveryCard({ body }: { body: string }) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const copy = useCallback(async () => {
    try {
      await clipboard.writeText(body);
      toast.success(t`已复制到剪贴板`);
    } catch (error) {
      toast.error(t`复制失败`, error);
    }
  }, [body, t]);

  return (
    <Surface className="gap-3 px-4 py-4" testID="inbox-detail-text-delivery">
      <View className="flex-row items-center justify-between gap-3">
        <View className="flex-row items-center gap-2">
          <ClipboardList color={colors.mutedForeground} size={15} />
          <Text className="text-[13px] font-medium text-muted-foreground">
            <Trans>文本内容</Trans>
          </Text>
        </View>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={t`复制文本`}
          onPress={() => void copy()}
          className="rounded-lg border border-border px-3 py-1.5 active:opacity-70"
        >
          <Text className="text-[12px] font-semibold text-foreground">
            <Trans>复制</Trans>
          </Text>
        </Pressable>
      </View>
      <Text selectable className="text-[14px] leading-6 text-foreground">
        {body}
      </Text>
    </Surface>
  );
}

/**
 * 无大预览时,标题区行首的内容类型 chip(与收件箱列表/文件行同一 chip 语言):
 * 缺失 → 警示色,多文件 → 合集,单文件 → 按扩展名。
 */
function TypeChip({
  missing,
  multi,
  primaryFile,
}: {
  missing: boolean;
  multi: boolean;
  primaryFile: InboxFileEntry | null;
}) {
  const colors = useThemeColors();
  const Icon = missing
    ? FileWarning
    : multi
      ? Package
      : primaryFile
        ? fileBrowserIcon(primaryFile.name)
        : Inbox;
  return (
    <View
      className={cn(
        "size-14 items-center justify-center rounded-xl",
        missing ? "bg-destructive/10" : "bg-muted",
      )}
    >
      <Icon
        color={missing ? colors.destructive : colors.foreground}
        size={24}
      />
    </View>
  );
}

function DetailActionBar({
  primaryFile,
  hasFolder,
  onOpen,
  onOpenFolder,
}: {
  primaryFile: InboxFileEntry | null;
  hasFolder: boolean;
  onOpen: () => void;
  onOpenFolder: () => void;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const canOpen = primaryFile != null && !primaryFile.missing;
  // 多文件且平台打不开保存目录(Android 私有目录):没有可用主动作,整栏不渲染
  // (逐项打开/分享由列表行承载,归档/删除在右上角菜单)。
  if (!primaryFile && !hasFolder) return null;
  return (
    <BottomActionBar testID="inbox-detail-action-bar">
      <Pressable
        accessibilityRole="button"
        onPress={primaryFile ? onOpen : onOpenFolder}
        disabled={primaryFile ? !canOpen : false}
        testID={primaryFile ? "inbox-file-share-0" : "inbox-open-folder"}
        className="min-h-12 flex-1 flex-row items-center justify-center gap-2 rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-50"
      >
        {primaryFile ? (
          <Eye color={colors.primaryForeground} size={18} />
        ) : (
          <FolderOpen color={colors.primaryForeground} size={18} />
        )}
        <Text className="text-[14px] font-semibold text-primary-foreground">
          {primaryFile ? <Trans>打开</Trans> : <Trans>打开文件夹</Trans>}
        </Text>
      </Pressable>
      {primaryFile && hasFolder ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={t`打开文件夹`}
          onPress={onOpenFolder}
          hitSlop={8}
          testID="inbox-open-folder"
          className="size-12 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
        >
          <FolderOpen color={colors.foreground} size={18} />
        </Pressable>
      ) : null}
    </BottomActionBar>
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
    <View className="overflow-hidden rounded-lg border border-border bg-card">
      <Pressable
        accessibilityRole="button"
        onPress={onToggle}
        className="min-h-12 flex-row items-center justify-between px-3.5 py-3 active:opacity-70"
      >
        <Text className="text-[14px] font-semibold text-foreground">
          <Trans>详情</Trans>
        </Text>
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
          icon={Tag}
          label={<Trans>来源类型</Trans>}
          value={<SourceKindLabel kind={detail.item.sourceKind} />}
        />
        {expanded ? (
          <>
            {/* 长路径/长 ID 这类核对用途的技术字段收在展开态,默认不占版面。 */}
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
            ? "text-[11px] font-medium text-destructive-ink"
            : "text-[11px] font-medium text-muted-foreground"
        }
      >
        {label}
      </Text>
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
        <Text className="text-[12px] text-muted-foreground">{label}</Text>
        <Text
          className={cn(
            "mt-0.5 text-[13px] text-foreground",
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

const detailStyles = StyleSheet.create({
  previewFrame: {
    aspectRatio: 1.08,
  },
  videoSurface: {
    width: "100%",
    height: "100%",
  },
});

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
