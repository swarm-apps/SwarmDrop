import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams, useRouter } from "expo-router";
import {
  CheckCircle2,
  FolderOpen,
  type LucideIcon,
  Pause,
  Play,
  Send,
  Trash2,
  X,
  XCircle,
} from "lucide-react-native";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import {
  MobileSaveLocation_Tags,
  type MobileTransferProjection,
} from "react-native-swarmdrop-core";
import { buildTreeDataFromOffer, FileTree } from "@/components/file-tree";
import { KeyValueRow } from "@/components/key-value-row";
import {
  BottomActionBar,
  HeaderIconButton,
  Surface,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import {
  calcPercent,
  canResend,
  canResume,
  DirectionIcon,
  formatBytes,
  formatSpeed,
  LocalizedError,
  ProgressBar,
  projectionReasonLabel,
  StatusBadge,
  StatusLabel,
} from "@/components/transfer/shared";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { canOpenSaveFolder } from "@/core/saf-intent";
import {
  isProjectionActive,
  isProjectionRecoverable,
  projectionDirection,
  projectionPolicyNote,
  projectionStatus,
  projectionTotalBytes,
  projectionTransferredBytes,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { openSaveFolderOrToast } from "@/lib/save-folder";
import { toast } from "@/lib/toast";
import { errorMessage, lastPathSegment, truncateMiddle } from "@/lib/utils";
import { useTransferStore } from "@/stores/transfer-store";

export default function TransferDetailScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const { sessionId } = useLocalSearchParams<{ sessionId: string }>();
  const projection = useTransferStore((s) =>
    sessionId ? s.projections[sessionId] : undefined,
  );
  const progress = useTransferStore((s) =>
    sessionId ? s.progressBySession[sessionId] : undefined,
  );
  const loadProjection = useTransferStore((s) => s.loadProjection);
  const deleteHistoryItem = useTransferStore((s) => s.deleteHistoryItem);
  const resumeHistoryItem = useTransferStore((s) => s.resumeHistoryItem);
  const refreshAfterTransition = useTransferStore(
    (s) => s.refreshAfterTransition,
  );

  const [busy, setBusy] = useState<
    null | "pausing" | "cancelling" | "resuming" | "deleting"
  >(null);
  const [loaded, setLoaded] = useState(false);
  const [cancelOpen, setCancelOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    void (async () => {
      await loadProjection(sessionId);
      if (!cancelled) setLoaded(true);
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionId, loadProjection]);

  const onPause = useCallback(async () => {
    if (!sessionId || busy) return;
    setBusy("pausing");
    try {
      await getMobileCore().pauseTransfer(sessionId);
      await refreshAfterTransition(sessionId);
    } catch (err) {
      toast.error(t`暂停失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [sessionId, busy, refreshAfterTransition, t]);

  const performCancel = useCallback(async () => {
    if (!sessionId) return;
    setBusy("cancelling");
    try {
      await getMobileCore().cancelTransfer(sessionId);
      await refreshAfterTransition(sessionId);
      setCancelOpen(false);
      toast.success(t`已取消传输`);
      router.replace("/transfer" as never);
    } catch (err) {
      toast.error(t`取消失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [sessionId, refreshAfterTransition, router, t]);

  const onResume = useCallback(async () => {
    if (!sessionId || busy) return;
    setBusy("resuming");
    try {
      const nextId = await resumeHistoryItem(sessionId);
      if (nextId !== sessionId) {
        router.replace({
          pathname: "/transfer/[sessionId]",
          params: { sessionId: nextId },
        } as never);
      }
    } catch (err) {
      toast.error(t`恢复失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [sessionId, busy, resumeHistoryItem, router, t]);

  const performDelete = useCallback(async () => {
    if (!sessionId) return;
    setBusy("deleting");
    try {
      await deleteHistoryItem(sessionId);
      router.back();
    } catch (err) {
      toast.error(t`删除失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [sessionId, deleteHistoryItem, router, t]);

  // 重新发送:失败的发送在核心里没有 resend API、投影也不含文件句柄,
  // 只能诚实地回到发送流程(预选好该设备)让用户重新挑文件,而不是假装能一键重发。
  const onResend = useCallback(() => {
    const peerId = projection?.peerId;
    if (!peerId) return;
    router.push({
      pathname: "/send/select-device",
      params: { peerId },
    } as never);
  }, [router, projection?.peerId]);

  const savePath = projection ? savePathOf(projection) : null;
  const isActive = projection != null && isProjectionActive(projection);

  const openFolder = useCallback(() => {
    if (!savePath) return;
    void openSaveFolderOrToast(savePath);
  }, [savePath]);

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader
        title={t`传输详情`}
        // 删除是低频动作,收进右上角(与收件箱详情的「更多」同位),不占底部动作栏
        right={
          projection && !isActive ? (
            <HeaderIconButton
              icon={Trash2}
              label={t`删除记录`}
              onPress={() => setDeleteOpen(true)}
              testID="transfer-delete-button"
            />
          ) : null
        }
      />
      <ScrollView contentContainerClassName="gap-5 px-5 pt-2 pb-8">
        {!projection ? (
          loaded ? (
            <View className="items-center gap-3 py-20">
              <Text className="text-xs text-muted-foreground">
                <Trans>会话不存在或已结束</Trans>
              </Text>
            </View>
          ) : (
            <TransferDetailSkeleton label={t`加载中`} />
          )
        ) : (
          <TransferDetailContent projection={projection} progress={progress} />
        )}
      </ScrollView>

      {projection ? (
        <TransferActionBar
          projection={projection}
          busy={busy}
          onPause={onPause}
          onCancel={() => setCancelOpen(true)}
          onResume={onResume}
          onResend={onResend}
          onOpenFolder={openFolder}
        />
      ) : null}

      <ConfirmDialog
        open={cancelOpen}
        onOpenChange={setCancelOpen}
        title={<Trans>取消传输</Trans>}
        description={<Trans>确定要中止这次传输吗？</Trans>}
        cancelLabel={<Trans>继续</Trans>}
        actionLabel={<Trans>中止</Trans>}
        destructive
        onAction={performCancel}
      />

      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={<Trans>删除这条记录</Trans>}
        description={<Trans>仅从本机活动记录中删除，不影响对端。</Trans>}
        actionLabel={<Trans>删除</Trans>}
        destructive
        onAction={performDelete}
      />
    </SafeAreaView>
  );
}

// 加载骨架:镜像 TransferDetailContent 的主要区块(状态头/进度块/信息卡/文件列表)
function TransferDetailSkeleton({ label }: { label: string }) {
  return (
    <View className="gap-5" accessible accessibilityLabel={label}>
      {/* 状态头:方向图标 chip + 两行文本 + 状态徽章 */}
      <View className="flex-row items-center gap-3">
        <Skeleton className="size-10 rounded-xl" />
        <View className="min-w-0 flex-1 gap-1.5">
          <Skeleton className="h-3.5 w-1/2" />
          <Skeleton className="h-3 w-2/3" />
        </View>
        <Skeleton className="h-5 w-14 rounded-full" />
      </View>

      {/* 状态横幅:chip + 两行文本 */}
      <View className="flex-row items-center gap-3 rounded-lg border border-border bg-card p-4">
        <Skeleton className="size-11 rounded-full" />
        <View className="min-w-0 flex-1 gap-1.5">
          <Skeleton className="h-3.5 w-1/3" />
          <Skeleton className="h-3 w-1/2" />
        </View>
      </View>

      {/* 信息卡:与真实卡片相同 chrome */}
      <View className="gap-3 rounded-lg border border-border bg-card p-4">
        <View className="flex-row items-center justify-between gap-3">
          <Skeleton className="h-3 w-16" />
          <Skeleton className="h-3 w-1/2" />
        </View>
        <View className="flex-row items-center justify-between gap-3">
          <Skeleton className="h-3 w-12" />
          <Skeleton className="h-3 w-1/3" />
        </View>
        <View className="flex-row items-center justify-between gap-3">
          <Skeleton className="h-3 w-16" />
          <Skeleton className="h-3 w-2/3" />
        </View>
      </View>

      {/* 文件列表 */}
      <View className="gap-2">
        <Skeleton className="h-3.5 w-16" />
        <View className="gap-2">
          <View className="flex-row items-center gap-2.5">
            <Skeleton className="size-5" />
            <Skeleton className="h-3 w-2/3" />
          </View>
          <View className="flex-row items-center gap-2.5">
            <Skeleton className="size-5" />
            <Skeleton className="h-3 w-1/2" />
          </View>
          <View className="flex-row items-center gap-2.5">
            <Skeleton className="size-5" />
            <Skeleton className="h-3 w-1/3" />
          </View>
        </View>
      </View>
    </View>
  );
}

function TransferDetailContent({
  projection,
  progress,
}: {
  projection: MobileTransferProjection;
  progress: Parameters<typeof projectionTransferredBytes>[1];
}) {
  const status = projectionStatus(projection);
  const direction = projectionDirection(projection);

  const treeData = useMemo(
    () =>
      buildTreeDataFromOffer(
        projection.files.map((file) => ({
          fileId: file.fileId,
          name: file.name,
          relativePath: file.relativePath || file.name,
          size: Number(file.size),
        })),
      ),
    [projection.files],
  );

  return (
    <>
      <View className="flex-row items-center gap-3">
        <DirectionIcon direction={direction} />
        <View className="min-w-0 flex-1">
          <Text
            className="text-base font-semibold text-foreground"
            numberOfLines={1}
          >
            {projection.peerName}
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            {direction === "send" ? <Trans>发送</Trans> : <Trans>接收</Trans>}
            {" · "}
            {projection.files.length} <Trans>个文件</Trans>
            {" · "}
            {formatBytes(projection.totalSize)}
          </Text>
        </View>
        <StatusBadge status={status} />
      </View>

      <TransferProgressBlock projection={projection} progress={progress} />

      <DetailCard projection={projection} />

      <View className="gap-2">
        <View className="flex-row items-baseline justify-between">
          <Text className="text-[13px] font-semibold text-muted-foreground">
            <Trans>文件</Trans>
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            {status === "transferring" && progress ? (
              `${progress.completedFiles}/${projection.files.length}`
            ) : (
              <Trans>{projection.files.length} 项</Trans>
            )}
          </Text>
        </View>
        <FileTree
          mode={status === "transferring" ? "transfer" : "select"}
          dataLoader={treeData.dataLoader}
          rootChildren={treeData.rootChildren}
          progress={progress ?? null}
        />
      </View>
    </>
  );
}

/**
 * 详情信息卡:内容全部源自 projection(tick 间引用稳定,仅相变时替换),
 * memo 让每秒多次的进度 tick 不再重跑 toLocaleString / decode 等格式化。
 */
const DetailCard = memo(function DetailCard({
  projection,
}: {
  projection: MobileTransferProjection;
}) {
  const savePath = savePathOf(projection);
  const policyNote = projectionPolicyNote(projection);
  return (
    <View className="gap-2">
      <Text className="text-[13px] font-semibold text-muted-foreground">
        <Trans>详情</Trans>
      </Text>
      <Surface className="gap-3 p-4">
        <KeyValueRow
          label={<Trans>开始时间</Trans>}
          value={new Date(Number(projection.startedAt)).toLocaleString()}
        />
        {projection.finishedAt != null ? (
          <KeyValueRow
            label={<Trans>结束时间</Trans>}
            value={new Date(Number(projection.finishedAt)).toLocaleString()}
          />
        ) : null}
        <KeyValueRow
          label={<Trans>对端</Trans>}
          value={truncateMiddle(projection.peerId, 6, 4)}
          mono
        />
        <KeyValueRow
          label={<Trans>加密</Trans>}
          value={<Trans>端到端 · 本次传输专用密钥</Trans>}
        />
        {savePath ? (
          <KeyValueRow
            label={<Trans>保存位置</Trans>}
            value={`…/${lastPathSegment(savePath)}`}
            mono
          />
        ) : null}
        {policyNote ? (
          <KeyValueRow label={<Trans>设备策略</Trans>} value={policyNote} />
        ) : null}
      </Surface>
    </View>
  );
});

function TransferProgressBlock({
  projection,
  progress,
}: {
  projection: MobileTransferProjection;
  progress: Parameters<typeof projectionTransferredBytes>[1];
}) {
  const status = projectionStatus(projection);
  const colors = useThemeColors();
  const transferred = projectionTransferredBytes(projection, progress);
  const total = projectionTotalBytes(projection, progress);
  const percent = calcPercent(transferred, total);

  if (status === "transferring" || status === "paused") {
    return (
      <View className="gap-2">
        <View className="flex-row items-baseline justify-between">
          <Text className="text-3xl font-bold tabular-nums text-foreground">
            {percent}%
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            {status === "transferring" && progress ? (
              formatSpeed(Number(progress.speed))
            ) : (
              <StatusLabel status={status} />
            )}
          </Text>
        </View>
        <ProgressBar percent={percent} />
        <Text className="text-[11px] text-muted-foreground">
          {formatBytes(transferred)} / {formatBytes(total)}
        </Text>
      </View>
    );
  }

  if (status === "completed") {
    const duration =
      projection.finishedAt != null
        ? Math.max(
            0,
            Math.round(
              (Number(projection.finishedAt) - Number(projection.startedAt)) /
                1000,
            ),
          )
        : 0;
    // 平均速度是这块唯一不与头部/文件区重复的新信息(文件数/总大小已在头部副行)。
    const avgSpeed = duration > 0 ? Number(total) / duration : null;
    return (
      <StatusBanner
        chip={
          <View className="size-11 items-center justify-center rounded-full bg-success/15">
            <CheckCircle2 size={22} color={colors.success} strokeWidth={2.25} />
          </View>
        }
        title={<Trans>传输完成</Trans>}
        subtitle={
          avgSpeed != null ? (
            <Trans>
              用时 {formatDuration(duration)} · 平均 {formatSpeed(avgSpeed)}
            </Trans>
          ) : (
            <Trans>用时 {formatDuration(duration)}</Trans>
          )
        }
      />
    );
  }

  if (status === "failed") {
    return (
      <StatusBanner
        chip={
          <View className="size-11 items-center justify-center rounded-full bg-destructive/15">
            <XCircle size={22} color={colors.destructive} strokeWidth={2.25} />
          </View>
        }
        title={<Trans>传输失败</Trans>}
        subtitle={<LocalizedError message={projection.errorMessage} />}
      />
    );
  }

  // 等待对方确认(offered/waiting_accept):唯一仍在"进行"的非进度态,用 spinner 表达活跃。
  if (status === "offered" || status === "waiting_accept") {
    return (
      <StatusBanner
        chip={
          <View className="size-11 items-center justify-center rounded-full bg-muted">
            <ActivityIndicator size="small" color={colors.primary} />
          </View>
        }
        title={<StatusLabel status={status} />}
        subtitle={<Trans>对方确认后立即开始传输</Trans>}
      />
    );
  }

  // 其余挂起/终态(中断/离线/已取消/已拒绝):中性 chip,可恢复时给一句安心话。
  const Icon = status === "cancelled" || status === "rejected" ? X : Pause;
  return (
    <StatusBanner
      chip={
        <View className="size-11 items-center justify-center rounded-full bg-muted">
          <Icon size={20} color={colors.mutedForeground} strokeWidth={2.25} />
        </View>
      }
      title={
        projectionReasonLabel(projection) ?? <StatusLabel status={status} />
      }
      subtitle={
        isProjectionRecoverable(projection) ? (
          <Trans>可从断点继续，无需重新传输</Trans>
        ) : null
      }
    />
  );
}

/**
 * 终态/等待态的状态横幅:与全页左对齐网格统一(取代旧的居中堆叠),
 * 状态色只花在 chip 上,文字保持中性 —— 头部 StatusBadge 已承载彩色状态文字。
 */
function StatusBanner({
  chip,
  title,
  subtitle,
}: {
  chip: React.ReactNode;
  title: React.ReactNode;
  subtitle?: React.ReactNode;
}) {
  return (
    <Surface className="flex-row items-center gap-3 p-4">
      {chip}
      <View className="min-w-0 flex-1 gap-0.5">
        <Text className="text-[15px] font-semibold text-foreground">
          {title}
        </Text>
        {subtitle ? (
          <Text className="text-[12px] leading-4 text-muted-foreground">
            {subtitle}
          </Text>
        ) : null}
      </View>
    </Surface>
  );
}

/**
 * 固定底部动作栏 —— 与收件箱详情的 DetailActionBar 同一交互位与结构:
 * 主动作横排 flex-1,删除已收进右上角,取消(危险)以描边形态与主动作并排。
 */
function TransferActionBar({
  projection,
  busy,
  onPause,
  onCancel,
  onResume,
  onResend,
  onOpenFolder,
}: {
  projection: MobileTransferProjection;
  busy: string | null;
  onPause: () => void;
  onCancel: () => void;
  onResume: () => void;
  onResend: () => void;
  onOpenFolder: () => void;
}) {
  const actions: React.ReactNode[] = [];
  const status = projectionStatus(projection);
  const isActive = isProjectionActive(projection);
  const savePath = savePathOf(projection);

  if (isActive) {
    if (status === "transferring") {
      actions.push(
        <ActionButton
          key="pause"
          Icon={Pause}
          label={<Trans>暂停</Trans>}
          onPress={onPause}
          disabled={busy === "pausing"}
          loading={busy === "pausing"}
          variant="secondary"
        />,
      );
    }
    actions.push(
      <ActionButton
        key="cancel"
        Icon={X}
        label={<Trans>取消</Trans>}
        onPress={onCancel}
        disabled={busy === "cancelling"}
        loading={busy === "cancelling"}
        variant="destructive"
      />,
    );
  }

  if (canResume(projection)) {
    actions.push(
      <ActionButton
        key="resume"
        Icon={Play}
        label={<Trans>恢复</Trans>}
        onPress={onResume}
        disabled={busy === "resuming"}
        loading={busy === "resuming"}
        variant="primary"
      />,
    );
  }

  // canOpenSaveFolder=false(Android 私有目录)时不给一个必败按钮;文件去收件箱打开/分享。
  if (savePath && status === "completed" && canOpenSaveFolder(savePath)) {
    actions.push(
      <ActionButton
        key="open-folder"
        Icon={FolderOpen}
        label={<Trans>打开文件夹</Trans>}
        onPress={onOpenFolder}
        variant="primary"
      />,
    );
  }

  // 失败且不可续传的发送:给一条明确的向前出路(重新发送),而不是只剩"删除"死胡同。
  if (
    !isActive &&
    !canResume(projection) &&
    canResend(projection) &&
    status === "failed"
  ) {
    actions.push(
      <ActionButton
        key="resend"
        Icon={Send}
        label={<Trans>重新发送</Trans>}
        onPress={onResend}
        variant="primary"
      />,
    );
  }

  if (actions.length === 0) return null;
  return (
    <BottomActionBar testID="transfer-action-bar">{actions}</BottomActionBar>
  );
}

interface ActionButtonProps {
  Icon: LucideIcon;
  label: React.ReactNode;
  onPress: () => void;
  disabled?: boolean;
  loading?: boolean;
  variant: "primary" | "secondary" | "destructive";
}

function ActionButton({
  Icon,
  label,
  onPress,
  disabled,
  loading,
  variant,
}: ActionButtonProps) {
  const colors = useThemeColors();
  // 横排底栏三形态:主动作实心蓝,次动作描边白,破坏性描边白+红字(不做红色实心 hero)
  const style = {
    primary: {
      bg: "bg-primary",
      text: "text-primary-foreground",
      icon: colors.primaryForeground,
    },
    secondary: {
      bg: "border border-border bg-card",
      text: "text-foreground",
      icon: colors.foreground,
    },
    destructive: {
      bg: "border border-border bg-card",
      text: "text-destructive-ink",
      icon: colors.destructive,
    },
  }[variant];

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      className={`min-h-12 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl px-4 ${style.bg} active:opacity-70 disabled:opacity-50`}
    >
      {loading ? (
        <ActivityIndicator size="small" color={style.icon} />
      ) : (
        <Icon size={16} color={style.icon} />
      )}
      <Text className={`text-[13px] font-semibold ${style.text}`}>{label}</Text>
    </Pressable>
  );
}

/**
 * 「打开文件夹」/「保存位置」定位的目标目录:优先用 core 给的真实容器目录
 * `contentRoot`(收到内容实际所在文件夹,SAF/file:// 皆为合法可打开目录 URI);
 * 缺失(旧数据 / 发送会话 / 异常)回退配置的存储根 `saveLocation`。
 * 绝不用 saveLocation + relativePath 拼接推导(SAF/重名下失真)。
 */
function savePathOf(projection: MobileTransferProjection): string | null {
  if (projection.contentRoot) {
    return projection.contentRoot;
  }
  if (projection.saveLocation?.tag !== MobileSaveLocation_Tags.Path) {
    return null;
  }
  return projection.saveLocation.inner.path;
}

function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "-";
  if (seconds < 60) return `${Math.ceil(seconds)}s`;
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = Math.ceil(seconds % 60);
    return `${m}m ${s}s`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.ceil((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}
