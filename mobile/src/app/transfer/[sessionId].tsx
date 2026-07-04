import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams, useRouter } from "expo-router";
import {
  CheckCircle2,
  FolderOpen,
  Loader2,
  type LucideIcon,
  Pause,
  Play,
  Send,
  Trash2,
  X,
  XCircle,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import {
  MobileSaveLocation_Tags,
  MobileTransferPhase,
  type MobileTransferProjection,
} from "react-native-swarmdrop-core";
import { buildTreeDataFromOffer, FileTree } from "@/components/file-tree";
import { KeyValueRow } from "@/components/key-value-row";
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
import { openSafTreeUri } from "@/core/saf-intent";
import {
  projectionDirection,
  projectionPolicyNote,
  projectionStatus,
  projectionTotalBytes,
  projectionTransferredBytes,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { errorMessage, truncateMiddle } from "@/lib/utils";
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

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`传输详情`} />
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
          <TransferDetailContent
            projection={projection}
            progress={progress}
            busy={busy}
            onPause={onPause}
            onCancel={() => setCancelOpen(true)}
            onResume={onResume}
            onResend={onResend}
            onDelete={() => setDeleteOpen(true)}
          />
        )}
      </ScrollView>

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

      {/* 进度块:大百分比数字 + 进度条 + 字节数 */}
      <View className="gap-2">
        <View className="flex-row items-end justify-between">
          <Skeleton className="h-8 w-24" />
          <Skeleton className="h-3 w-1/4" />
        </View>
        <Skeleton className="h-2 w-full rounded-full" />
        <Skeleton className="h-3 w-1/3" />
      </View>

      {/* 信息卡:与真实卡片相同 chrome */}
      <View className="gap-3 rounded-xl border border-border bg-card p-4">
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
  busy,
  onPause,
  onCancel,
  onResume,
  onResend,
  onDelete,
}: {
  projection: MobileTransferProjection;
  progress: Parameters<typeof projectionTransferredBytes>[1];
  busy: string | null;
  onPause: () => void;
  onCancel: () => void;
  onResume: () => void;
  onResend: () => void;
  onDelete: () => void;
}) {
  const status = projectionStatus(projection);
  const direction = projectionDirection(projection);
  const savePath = savePathOf(projection);
  const isActive =
    projection.phase === MobileTransferPhase.Offered ||
    projection.phase === MobileTransferPhase.WaitingAccept ||
    projection.phase === MobileTransferPhase.Active;

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

  const openFolder = useCallback(async () => {
    if (!savePath) return;
    try {
      await openSafTreeUri(savePath);
    } catch {
      toast.info(savePath);
    }
  }, [savePath]);

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

      <View className="gap-3 rounded-xl border border-border bg-card p-4">
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
            value={decodeURIComponent(savePath)}
            mono
          />
        ) : null}
        {projectionPolicyNote(projection) ? (
          <KeyValueRow
            label={<Trans>设备策略</Trans>}
            value={projectionPolicyNote(projection)}
          />
        ) : null}
      </View>

      <View className="gap-2">
        <Text className="text-[13px] font-semibold text-muted-foreground">
          <Trans>文件</Trans>
        </Text>
        <FileTree
          mode={status === "transferring" ? "transfer" : "select"}
          dataLoader={treeData.dataLoader}
          rootChildren={treeData.rootChildren}
          totalCount={projection.files.length}
          totalSize={Number(projection.totalSize)}
          progress={progress ?? null}
        />
      </View>

      <ActionBar
        projection={projection}
        busy={busy}
        isActive={isActive}
        savePath={savePath}
        onPause={onPause}
        onCancel={onCancel}
        onResume={onResume}
        onResend={onResend}
        onDelete={onDelete}
        onOpenFolder={openFolder}
      />
    </>
  );
}

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
    return (
      <View className="items-center gap-3 py-2">
        <View className="size-14 items-center justify-center rounded-full bg-success/15">
          <CheckCircle2 size={32} color={colors.success} strokeWidth={2} />
        </View>
        <Text className="text-base font-semibold text-foreground">
          <Trans>所有文件传输完成！</Trans>
        </Text>
        <View className="w-full max-w-xs flex-row justify-around px-4">
          <Stat
            value={String(projection.files.length)}
            label={<Trans>文件</Trans>}
          />
          <Stat
            value={formatBytes(projection.totalSize)}
            label={<Trans>总大小</Trans>}
          />
          <Stat value={formatDuration(duration)} label={<Trans>用时</Trans>} />
        </View>
      </View>
    );
  }

  if (status === "failed") {
    return (
      <View className="items-center gap-3 py-2">
        <View className="size-14 items-center justify-center rounded-full bg-destructive/15">
          <XCircle size={32} color={colors.destructive} strokeWidth={2} />
        </View>
        <Text className="text-base font-semibold text-foreground">
          <Trans>传输失败</Trans>
        </Text>
        <Text className="max-w-xs text-center text-[11px] text-muted-foreground">
          <LocalizedError message={projection.errorMessage} />
        </Text>
      </View>
    );
  }

  const reason = projectionReasonLabel(projection);
  return (
    <View className="items-center gap-2 py-4">
      <Loader2 size={24} color={colors.primary} />
      <Text className="text-[11px] text-muted-foreground">
        {reason ?? <StatusLabel status={status} />}
      </Text>
    </View>
  );
}

function ActionBar({
  projection,
  busy,
  isActive,
  savePath,
  onPause,
  onCancel,
  onResume,
  onResend,
  onDelete,
  onOpenFolder,
}: {
  projection: MobileTransferProjection;
  busy: string | null;
  isActive: boolean;
  savePath: string | null;
  onPause: () => void;
  onCancel: () => void;
  onResume: () => void;
  onResend: () => void;
  onDelete: () => void;
  onOpenFolder: () => void;
}) {
  // 两级层级:主操作(恢复/打开文件夹/暂停)全宽实心占主导;
  // 破坏性操作(取消/删除)降权为窄、居中、无填充,永不撑满成红色 hero。
  const prominent: React.ReactNode[] = [];
  const destructive: React.ReactNode[] = [];
  const status = projectionStatus(projection);

  if (isActive) {
    if (status === "transferring") {
      prominent.push(
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
    destructive.push(
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
    prominent.push(
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

  if (savePath && status === "completed") {
    prominent.push(
      <ActionButton
        key="open-folder"
        Icon={FolderOpen}
        label={<Trans>打开文件夹</Trans>}
        onPress={onOpenFolder}
        variant="secondary"
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
    prominent.push(
      <ActionButton
        key="resend"
        Icon={Send}
        label={<Trans>重新发送</Trans>}
        onPress={onResend}
        variant="primary"
      />,
    );
  }

  if (!isActive) {
    destructive.push(
      <ActionButton
        key="delete"
        Icon={Trash2}
        label={<Trans>删除</Trans>}
        onPress={onDelete}
        disabled={busy === "deleting"}
        loading={busy === "deleting"}
        variant="destructive"
      />,
    );
  }

  if (prominent.length === 0 && destructive.length === 0) return null;
  return (
    <View className="gap-2 pt-1">
      {prominent.length > 0 ? <View className="gap-2">{prominent}</View> : null}
      {destructive.length > 0 ? (
        <View className="flex-row flex-wrap items-center justify-center gap-x-4 gap-y-1">
          {destructive}
        </View>
      ) : null}
    </View>
  );
}

function Stat({ value, label }: { value: string; label: React.ReactNode }) {
  return (
    <View className="items-center gap-0.5">
      <Text className="text-lg font-bold tabular-nums text-foreground">
        {value}
      </Text>
      <Text className="text-[10px] text-muted-foreground">{label}</Text>
    </View>
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
  const isDestructive = variant === "destructive";
  const bgClass =
    variant === "primary"
      ? "bg-primary"
      : variant === "secondary"
        ? "border border-border bg-card"
        : "active:bg-destructive/10"; // destructive: 无填充,降权
  const textClass =
    variant === "primary"
      ? "text-primary-foreground"
      : variant === "secondary"
        ? "text-foreground"
        : "text-destructive-ink";
  const iconColor =
    variant === "primary"
      ? colors.primaryForeground
      : variant === "secondary"
        ? colors.foreground
        : colors.destructive;
  // 主/次操作:全宽实心,占主导;破坏性:窄、内容自适应宽度,让位于主操作
  const layoutClass = isDestructive
    ? "min-h-11 px-4 py-2"
    : "min-h-12 w-full py-3";

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      className={`flex-row items-center justify-center gap-1.5 rounded-xl ${layoutClass} ${bgClass} active:opacity-70 disabled:opacity-50`}
    >
      {loading ? (
        <ActivityIndicator size="small" color={iconColor} />
      ) : (
        <Icon size={16} color={iconColor} />
      )}
      <Text className={`text-[13px] font-medium ${textClass}`}>{label}</Text>
    </Pressable>
  );
}

function savePathOf(projection: MobileTransferProjection): string | null {
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
