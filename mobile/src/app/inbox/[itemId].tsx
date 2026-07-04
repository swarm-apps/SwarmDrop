import { Trans, useLingui } from "@lingui/react/macro";
import { File } from "expo-file-system";
import { useFocusEffect, useLocalSearchParams, useRouter } from "expo-router";
import * as Sharing from "expo-sharing";
import { useVideoPlayer, VideoView } from "expo-video";
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
  FileArchive,
  FileText,
  FileWarning,
  FolderOpen,
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
import { Image, Pressable, ScrollView, StyleSheet, View } from "react-native";
import ImageViewing from "react-native-image-viewing";
import {
  MobileInboxContentKind,
  type MobileInboxItemDetail,
  MobileInboxSourceKind,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
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
import { canOpenSaveFolder } from "@/core/saf-intent";
import { useThemeColors } from "@/hooks/useThemeColors";
import { openFileWithSystem } from "@/lib/open-file";
import { openSaveFolderOrToast } from "@/lib/save-folder";
import { toast } from "@/lib/toast";
import { cn, parentDirOf } from "@/lib/utils";
import { type InboxFileEntry, useInboxStore } from "@/stores/inbox-store";

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
  const title = detail?.item.title ?? t`收件箱详情`;
  const fileCount = detail?.files.length ?? 0;
  const primaryFile = detail?.files.length === 1 ? detail.files[0] : null;
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
        const available = await Sharing.isAvailableAsync();
        if (!available) {
          toast.error(t`系统分享不可用`);
          return;
        }
        await Sharing.shareAsync(file.localPath, {
          dialogTitle: t`分享文件`,
        });
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
          const available = await Sharing.isAvailableAsync();
          if (!available) {
            toast.error(t`没有应用能打开这个文件`);
            return;
          }
          await Sharing.shareAsync(file.localPath, {
            dialogTitle: t`分享文件`,
          });
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

  // 打开保存目录:优先记录的根目录,单文件回退到文件所在目录。
  // (曾是「复制路径」——一串 file://,普通用户拿到也无处可贴;直接带去文件管理器。)
  // canOpenSaveFolder=false(Android 私有目录)时入口整个不渲染,不给用户一个必败按钮。
  const folderTarget =
    detail?.item.rootPath ??
    (primaryFile ? parentDirOf(primaryFile.localPath) : null);
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

  return (
    <AppScreen testID="inbox-detail-screen" contentClassName="px-0 pb-0 pt-0">
      <SettingsHeader
        title={t`收件箱详情`}
        right={detail ? <MoreButton onPress={openActionsSheet} /> : null}
      />

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
            <Text className="text-center text-[12px] text-muted-foreground">
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
            {previewImageFile ? (
              <ImagePreview file={previewImageFile} />
            ) : previewVideoFile ? (
              <VideoPreview file={previewVideoFile} />
            ) : excerptFile ? (
              <TextExcerptCard
                kind={detail.item.contentKind}
                file={excerptFile}
              />
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
                    className="text-[12px] leading-5 text-muted-foreground"
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

            {detail.files.length > 1 ? (
              <IncludedFiles files={detail.files} onOpenFile={openFile} />
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
                className="min-h-12 flex-row items-center justify-between gap-3 rounded-lg border border-border bg-card px-3.5 active:opacity-70"
              >
                <Text className="text-[13px] font-semibold text-foreground">
                  <Trans>查看传输过程</Trans>
                </Text>
                <ExternalLink color={colors.mutedForeground} size={17} />
              </Pressable>
            ) : null}
          </ScrollView>

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
          <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
            {title}
          </Text>
        </View>

        <View className="overflow-hidden rounded-lg border border-border bg-background">
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
  const player = useVideoPlayer(file.localPath);
  // 路由失焦即暂停:expo-video 只在 app 退后台自动停,app 内导航跳走后
  // 本屏仍挂载,不暂停的话音频会跟到别的页面。
  useFocusEffect(
    useCallback(() => {
      return () => player.pause();
    }, [player]),
  );
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
        <Text className="text-[12px] font-medium text-muted-foreground">
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
        ? fileIcon(primaryFile.name)
        : FileArchive;
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

function IncludedFiles({
  files,
  onOpenFile,
}: {
  files: InboxFileEntry[];
  onOpenFile: (file: InboxFileEntry) => void;
}) {
  return (
    <View className="gap-2.5" testID="inbox-detail-files">
      {/* 计数不再重复:标题区副行已有「N 项 · 大小」 */}
      <Text className="text-[15px] font-semibold text-foreground">
        <Trans>包含内容</Trans>
      </Text>
      <View className="overflow-hidden rounded-lg border border-border bg-card">
        {files.map((file, index) => (
          <FileRow
            key={file.id}
            file={file}
            index={index}
            onOpenFile={onOpenFile}
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
  onOpenFile,
  separated = false,
}: {
  file: InboxFileEntry;
  index: number;
  onOpenFile: (file: InboxFileEntry) => void;
  separated?: boolean;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const Icon = fileIcon(file.name);
  // 相对路径只展示目录部分(尾段就是文件名,重复);平铺接收(无目录)时不展示。
  // checksum 属协议细节,不上一级界面。
  const pathDir = parentDirOf(file.relativePath);
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={t`打开 ${file.name}`}
      onPress={() => onOpenFile(file)}
      disabled={file.missing}
      testID={`inbox-file-share-${index}`}
      className={cn(
        "min-h-16 flex-row items-center gap-3 bg-card p-3 active:bg-muted",
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
      <View className="min-w-0 flex-1 gap-0.5">
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
          {formatBytes(file.size)}
          {pathDir ? ` · ${pathDir}` : null}
        </Text>
      </View>
      <ExternalLink color={colors.mutedForeground} size={16} />
    </Pressable>
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
            ? "text-[10px] font-medium text-destructive-ink"
            : "text-[10px] font-medium text-muted-foreground"
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

function fileIcon(name: string): LucideIcon {
  if (isImageFile(name)) return ImageIcon;
  if (isVideoFile(name)) return Video;
  return FileArchive;
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
  videoSurface: {
    width: "100%",
    height: "100%",
  },
});
