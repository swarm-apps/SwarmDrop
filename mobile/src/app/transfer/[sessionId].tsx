/**
 * 会话详情页 —— 优先从 store.sessions（活跃）、再 store.dbHistory（历史快照），
 * 最后退到 native `getTransferSessionDetail` 兜底（deep link / 历史很久之前的会话）。
 *
 * UI 形态对齐桌面端 `routes/_app/transfer/$sessionId.lazy.tsx`。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import { useLocalSearchParams, useRouter } from "expo-router";
import * as Sharing from "expo-sharing";
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
import type {
  MobileTransferHistoryItem,
  MobileTransferProgress,
} from "react-native-swarmdrop-core";
import {
  buildTreeDataFromOffer,
  FileTree,
  type TreeNodeData,
} from "@/components/file-tree";
import { SettingsHeader } from "@/components/settings-header";
import {
  calcPercent,
  canResend,
  canResume,
  canShareFile,
  DirectionIcon,
  formatBytes,
  formatSpeed,
  LocalizedError,
  ProgressBar,
  StatusBadge,
  StatusLabel,
  statusKey,
} from "@/components/transfer/shared";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { openSafTreeUri } from "@/core/saf-intent";
import type {
  ActiveStatus,
  TransferDirection,
  TransferSession,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { errorMessage } from "@/lib/utils";
import { useTransferStore } from "@/stores/transfer-store";

interface DetailViewModel {
  sessionId: string;
  direction: TransferDirection;
  peerId: string;
  peerName: string;
  files: { fileId: number; name: string; relativePath: string; size: number }[];
  totalSize: number;
  transferredBytes: number;
  status: ActiveStatus;
  progress: MobileTransferProgress | null;
  /** 字段名避开 lib/utils 同名工具 errorMessage 的 shadow */
  error: string | null;
  /** 接收方的保存路径（content:// / file://），发送方为 null */
  savePath: string | null;
  startedAt: number;
  finishedAt: number | null;
  fromHistory: boolean;
  /** 历史 item 原始对象；canShareFile/canResume/canResend 还在这里取 */
  historyItem: MobileTransferHistoryItem | null;
}

function fromActiveSession(s: TransferSession): DetailViewModel {
  return {
    sessionId: s.sessionId,
    direction: s.direction,
    peerId: s.peerId,
    peerName: s.peerName,
    files: s.files.map((f) => ({
      fileId: f.fileId,
      name: f.name,
      relativePath: f.relativePath || f.name,
      size: Number(f.size),
    })),
    totalSize: Number(s.totalSize),
    transferredBytes: s.progress ? Number(s.progress.transferredBytes) : 0,
    status: s.status,
    progress: s.progress,
    error: s.error,
    savePath: null,
    startedAt: s.startedAt,
    finishedAt: s.completedAt,
    fromHistory: false,
    historyItem: null,
  };
}

function fromHistoryItem(h: MobileTransferHistoryItem): DetailViewModel {
  // statusKey 把 native enum → 字面量；"unknown" 兜成 "transferring" 是安全默认。
  const literal = statusKey(h.status);
  const status: ActiveStatus =
    literal === "unknown" ? "transferring" : (literal as ActiveStatus);
  return {
    sessionId: h.sessionId,
    direction: h.direction === "send" ? "send" : "receive",
    peerId: h.peerId,
    peerName: h.peerName,
    files: h.files.map((f) => ({
      fileId: f.fileId,
      name: f.name,
      relativePath: f.relativePath || f.name,
      size: Number(f.size),
    })),
    totalSize: Number(h.totalSize),
    transferredBytes: Number(h.transferredBytes),
    status,
    progress: null,
    error: h.errorMessage ?? null,
    savePath: h.savePath ?? null,
    startedAt: Number(h.startedAt),
    finishedAt: h.finishedAt != null ? Number(h.finishedAt) : null,
    fromHistory: true,
    historyItem: h,
  };
}

type Load =
  | { kind: "view"; vm: DetailViewModel }
  | { kind: "loading" }
  | { kind: "not-found" };

export default function TransferDetailScreen() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const router = useRouter();
  const { sessionId } = useLocalSearchParams<{ sessionId: string }>();

  const session = useTransferStore((s) =>
    sessionId ? s.sessions[sessionId] : undefined,
  );
  const dbHistory = useTransferStore((s) => s.dbHistory);
  const deleteHistoryItem = useTransferStore((s) => s.deleteHistoryItem);
  const resumeHistoryItem = useTransferStore((s) => s.resumeHistoryItem);
  const startSend = useTransferStore((s) => s.startSend);

  const historyMatch = useMemo(
    () =>
      sessionId ? dbHistory.find((h) => h.sessionId === sessionId) : undefined,
    [dbHistory, sessionId],
  );

  // native 兜底：只在 sessionId 变更且既不在 sessions 也不在 dbHistory 时拉一次。
  const [fallback, setFallback] = useState<Load | null>(null);
  const fetchedForRef = useMemo(() => ({ id: null as string | null }), []);
  const [busy, setBusy] = useState<
    null | "pausing" | "cancelling" | "resuming" | "deleting" | "resending"
  >(null);
  const [cancelOpen, setCancelOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  useEffect(() => {
    if (!sessionId) return;
    if (session || historyMatch) {
      if (fallback !== null) setFallback(null);
      fetchedForRef.id = null;
      return;
    }
    if (fetchedForRef.id === sessionId) return;
    fetchedForRef.id = sessionId;

    setFallback({ kind: "loading" });
    let cancelled = false;
    void (async () => {
      try {
        const item = await getMobileCore().getTransferSessionDetail(sessionId);
        if (!cancelled) {
          setFallback({ kind: "view", vm: fromHistoryItem(item) });
        }
      } catch (err) {
        if (!cancelled) {
          console.warn("[transfer-detail] fallback fetch failed:", err);
          setFallback({ kind: "not-found" });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionId, session, historyMatch, fallback, fetchedForRef]);

  // 把 fromActiveSession/fromHistoryItem 包进 useMemo —— 它们每次都 .map 出
  // 新 files 数组，没有 useMemo 的话下游 buildTreeDataFromOffer/useMemo 永远
  // 不命中，每次 progress 事件都会重建整棵文件树。
  const vm = useMemo<DetailViewModel | null>(() => {
    if (session) return fromActiveSession(session);
    if (historyMatch) return fromHistoryItem(historyMatch);
    if (fallback?.kind === "view") return fallback.vm;
    return null;
  }, [session, historyMatch, fallback]);
  const load: Load = vm
    ? { kind: "view", vm }
    : (fallback ?? { kind: "loading" });

  const onPause = useCallback(async () => {
    if (!sessionId || busy) return;
    setBusy("pausing");
    try {
      await getMobileCore().pauseTransfer(sessionId);
    } catch (err) {
      toast.error(t`暂停失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [sessionId, busy, t]);

  const onCancel = useCallback(() => {
    if (!sessionId || busy) return;
    setCancelOpen(true);
  }, [sessionId, busy]);

  const performCancel = useCallback(async () => {
    if (!sessionId) return;
    setBusy("cancelling");
    try {
      await getMobileCore().cancelTransfer(sessionId);
    } catch (err) {
      toast.error(t`取消失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [sessionId, t]);

  const onResume = useCallback(async () => {
    if (!sessionId || busy) return;
    setBusy("resuming");
    try {
      const newId = await resumeHistoryItem(sessionId);
      if (newId !== sessionId) {
        router.replace({
          pathname: "/transfer/[sessionId]",
          params: { sessionId: newId },
        } as never);
      }
    } catch (err) {
      toast.error(t`恢复失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [sessionId, busy, resumeHistoryItem, router, t]);

  const onDelete = useCallback(() => {
    if (!sessionId || busy) return;
    setDeleteOpen(true);
  }, [sessionId, busy]);

  const performDelete = useCallback(async () => {
    if (!sessionId) return;
    setBusy("deleting");
    try {
      await deleteHistoryItem(sessionId);
      router.back();
    } catch (err) {
      toast.error(t`删除失败`, errorMessage(err));
      setBusy(null);
    }
  }, [sessionId, deleteHistoryItem, router, t]);

  const onResend = useCallback(async () => {
    if (load.kind !== "view" || !load.vm.historyItem || busy) return;
    const item = load.vm.historyItem;
    setBusy("resending");
    try {
      const newId = await startSend({
        files: item.files.map((f) => ({
          sourceId: f.relativePath,
          name: f.name,
          relativePath: f.relativePath,
          size: f.size,
        })),
        peerId: item.peerId,
        peerName: item.peerName,
      });
      router.replace({
        pathname: "/transfer/[sessionId]",
        params: { sessionId: newId },
      } as never);
    } catch (err) {
      toast.error(t`重新发送失败`, errorMessage(err));
    } finally {
      setBusy(null);
    }
  }, [load, busy, startSend, router, t]);

  const onShareFile = useCallback(
    async (uri: string, fileName: string) => {
      try {
        const available = await Sharing.isAvailableAsync();
        if (!available) {
          await Clipboard.setStringAsync(uri);
          toast.info(t`已复制保存路径`);
          return;
        }
        await Sharing.shareAsync(uri, { dialogTitle: fileName });
      } catch (err) {
        console.warn("[transfer-detail] share failed:", err);
        await Clipboard.setStringAsync(uri);
        toast.info(t`已复制保存路径`);
      }
    },
    [t],
  );

  const onCopyPath = useCallback(
    async (uri: string) => {
      await Clipboard.setStringAsync(uri);
      toast.success(t`已复制路径`);
    },
    [t],
  );

  const onOpenFolder = useCallback(
    async (savePath: string) => {
      try {
        await openSafTreeUri(savePath);
      } catch (err) {
        console.warn("[transfer-detail] open folder failed:", err);
        await Clipboard.setStringAsync(savePath);
        toast.info(t`已复制保存路径`);
      }
    },
    [t],
  );

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`传输详情`} />
      <ScrollView contentContainerClassName="gap-5 px-5 pt-2 pb-8">
        {load.kind === "loading" ? (
          <View className="items-center gap-3 py-20">
            <ActivityIndicator color={colors.mutedForeground} />
            <Text className="text-xs text-muted-foreground">
              <Trans>加载中</Trans>
            </Text>
          </View>
        ) : load.kind === "not-found" ? (
          <View className="items-center gap-2 py-20">
            <Text className="text-sm text-muted-foreground">
              <Trans>会话不存在或已结束</Trans>
            </Text>
          </View>
        ) : (
          <TransferDetailContent
            vm={load.vm}
            busy={busy}
            onPause={onPause}
            onCancel={onCancel}
            onResume={onResume}
            onResend={onResend}
            onDelete={onDelete}
            onShareFile={onShareFile}
            onCopyPath={onCopyPath}
            onOpenFolder={onOpenFolder}
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
        description={<Trans>仅从本机历史中删除，不影响对端。</Trans>}
        actionLabel={<Trans>删除</Trans>}
        destructive
        onAction={performDelete}
      />
    </SafeAreaView>
  );
}

interface DetailContentProps {
  vm: DetailViewModel;
  busy: string | null;
  onPause: () => void;
  onCancel: () => void;
  onResume: () => void;
  onResend: () => void;
  onDelete: () => void;
  onShareFile: (uri: string, fileName: string) => void;
  onCopyPath: (uri: string) => void;
  onOpenFolder: (savePath: string) => void;
}

function TransferDetailContent({
  vm,
  busy,
  onPause,
  onCancel,
  onResume,
  onResend,
  onDelete,
  onShareFile,
  onCopyPath,
  onOpenFolder,
}: DetailContentProps) {
  const status = vm.status;
  const isActive = status === "transferring" || status === "waiting_accept";

  const fileTreeMode: "select" | "transfer" =
    status === "transferring" ? "transfer" : "select";

  const treeData = useMemo(() => buildTreeDataFromOffer(vm.files), [vm.files]);

  // 拼装单文件 URI：savePath 是会话级目录，file.relativePath / name 才是实际文件
  const fileUriOf = useCallback(
    (data: TreeNodeData): string | null => {
      if (!vm.savePath) return null;
      const trimmed = vm.savePath.replace(/\/$/, "");
      const sub = data.path.replace(/^\/+/, "");
      return `${trimmed}/${sub || data.name}`;
    },
    [vm.savePath],
  );

  const shareEnabled = vm.historyItem ? canShareFile(vm.historyItem) : false;

  const handleFilePress = useCallback(
    (data: TreeNodeData) => {
      if (!shareEnabled) return;
      const uri = fileUriOf(data);
      if (uri) onShareFile(uri, data.name);
    },
    [shareEnabled, fileUriOf, onShareFile],
  );

  const handleFileLongPress = useCallback(
    (data: TreeNodeData) => {
      const uri = fileUriOf(data);
      if (uri) onCopyPath(uri);
    },
    [fileUriOf, onCopyPath],
  );

  return (
    <>
      <TransferStatusHeader vm={vm} />

      <TransferProgressBlock vm={vm} />

      {vm.fromHistory ? <HistoryMetaCard vm={vm} /> : null}

      <View className="gap-2">
        <Text className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          <Trans>传输详情</Trans>
        </Text>
        <FileTree
          mode={fileTreeMode}
          dataLoader={treeData.dataLoader}
          rootChildren={treeData.rootChildren}
          totalCount={vm.files.length}
          totalSize={vm.totalSize}
          progress={vm.progress}
          onFilePress={shareEnabled ? handleFilePress : undefined}
          onFileLongPress={vm.savePath ? handleFileLongPress : undefined}
        />
      </View>

      <ActionBar
        vm={vm}
        busy={busy}
        isActive={isActive}
        onPause={onPause}
        onCancel={onCancel}
        onResume={onResume}
        onResend={onResend}
        onDelete={onDelete}
        onOpenFolder={onOpenFolder}
      />
    </>
  );
}

function TransferStatusHeader({ vm }: { vm: DetailViewModel }) {
  const isSend = vm.direction === "send";
  return (
    <View className="flex-row items-center gap-3">
      <DirectionIcon direction={vm.direction} />
      <View className="min-w-0 flex-1">
        <Text
          className="text-base font-semibold text-foreground"
          numberOfLines={1}
        >
          {vm.peerName}
        </Text>
        <Text className="text-[11px] text-muted-foreground">
          {isSend ? <Trans>发送</Trans> : <Trans>接收</Trans>}
          {" · "}
          {vm.files.length} <Trans>个文件</Trans>
          {" · "}
          {formatBytes(vm.totalSize)}
        </Text>
      </View>
      {vm.status === "transferring" || vm.status === "waiting_accept" ? (
        <StatusBadge status={vm.status} />
      ) : null}
    </View>
  );
}

function TransferProgressBlock({ vm }: { vm: DetailViewModel }) {
  const status = vm.status;
  const colors = useThemeColors();

  if (status === "transferring" && vm.progress) {
    const percent = calcPercent(
      vm.progress.transferredBytes,
      vm.progress.totalBytes,
    );
    return (
      <View className="gap-2">
        <View className="flex-row items-baseline justify-between">
          <Text className="text-3xl font-bold tabular-nums text-foreground">
            {percent}%
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            {formatSpeed(Number(vm.progress.speed))}
          </Text>
        </View>
        <ProgressBar percent={percent} />
        <View className="flex-row items-center justify-between">
          <Text className="text-[11px] text-muted-foreground">
            {formatBytes(vm.progress.transferredBytes)} /{" "}
            {formatBytes(vm.progress.totalBytes)}
          </Text>
          {vm.progress.eta != null ? (
            <Text className="text-[11px] text-muted-foreground">
              <Trans>剩余 {formatDuration(Number(vm.progress.eta))}</Trans>
            </Text>
          ) : null}
        </View>
      </View>
    );
  }

  if (status === "paused") {
    const percent = calcPercent(vm.transferredBytes, vm.totalSize);
    return (
      <View className="gap-2">
        <View className="flex-row items-baseline justify-between">
          <Text className="text-3xl font-bold tabular-nums text-foreground">
            {percent}%
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            <Trans>已暂停</Trans>
          </Text>
        </View>
        <ProgressBar percent={percent} />
        <Text className="text-[11px] text-muted-foreground">
          {formatBytes(vm.transferredBytes)} / {formatBytes(vm.totalSize)}
        </Text>
      </View>
    );
  }

  if (status === "completed") {
    const duration =
      vm.finishedAt != null
        ? Math.max(0, Math.round((vm.finishedAt - vm.startedAt) / 1000))
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
          <Stat value={String(vm.files.length)} label={<Trans>文件</Trans>} />
          <Stat
            value={formatBytes(vm.totalSize)}
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
        {vm.error ? (
          <Text className="max-w-xs text-center text-[11px] text-muted-foreground">
            <LocalizedError message={vm.error} />
          </Text>
        ) : null}
      </View>
    );
  }

  if (status === "cancelled") {
    return (
      <View className="items-center gap-2 py-2">
        <Text className="text-sm text-muted-foreground">
          <StatusLabel status={status} />
        </Text>
      </View>
    );
  }

  if (status === "waiting_accept") {
    return (
      <View className="items-center gap-2 py-4">
        <Loader2 size={24} color={colors.primary} />
        <Text className="text-[11px] text-muted-foreground">
          <Trans>等待对方确认...</Trans>
        </Text>
      </View>
    );
  }

  return null;
}

function HistoryMetaCard({ vm }: { vm: DetailViewModel }) {
  return (
    <View className="gap-3 rounded-xl border border-border bg-card p-4">
      <DetailRow
        label={<Trans>开始时间</Trans>}
        value={new Date(vm.startedAt).toLocaleString()}
      />
      {vm.finishedAt != null ? (
        <DetailRow
          label={<Trans>结束时间</Trans>}
          value={new Date(vm.finishedAt).toLocaleString()}
        />
      ) : null}
      <DetailRow label={<Trans>对端</Trans>} value={vm.peerId} mono />
      {vm.savePath ? (
        <DetailRow
          label={<Trans>保存位置</Trans>}
          value={decodeURIComponent(vm.savePath)}
          mono
        />
      ) : null}
    </View>
  );
}

function ActionBar({
  vm,
  busy,
  isActive,
  onPause,
  onCancel,
  onResume,
  onResend,
  onDelete,
  onOpenFolder,
}: {
  vm: DetailViewModel;
  busy: string | null;
  isActive: boolean;
  onPause: () => void;
  onCancel: () => void;
  onResume: () => void;
  onResend: () => void;
  onDelete: () => void;
  onOpenFolder: (savePath: string) => void;
}) {
  const status = vm.status;
  const buttons: React.ReactNode[] = [];

  if (isActive) {
    if (status === "transferring") {
      buttons.push(
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
    buttons.push(
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
    return <View className="flex-row gap-2 pt-1">{buttons}</View>;
  }

  // 已结束 / 历史
  if (status === "completed" && vm.direction === "receive" && vm.savePath) {
    buttons.push(
      <ActionButton
        key="open-folder"
        Icon={FolderOpen}
        label={<Trans>打开文件夹</Trans>}
        onPress={() => onOpenFolder(vm.savePath as string)}
        variant="primary"
      />,
    );
  }

  if (vm.historyItem && canResume(vm.historyItem)) {
    buttons.push(
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

  if (vm.historyItem && canResend(vm.historyItem)) {
    buttons.push(
      <ActionButton
        key="resend"
        Icon={Send}
        label={<Trans>重新发送</Trans>}
        onPress={onResend}
        disabled={busy === "resending"}
        loading={busy === "resending"}
        variant="secondary"
      />,
    );
  }

  if (vm.fromHistory) {
    buttons.push(
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

  if (buttons.length === 0) return null;
  return <View className="flex-row flex-wrap gap-2 pt-1">{buttons}</View>;
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

function DetailRow({
  label,
  value,
  mono = false,
  danger = false,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  mono?: boolean;
  danger?: boolean;
}) {
  return (
    <View className="flex-row items-start justify-between gap-3">
      <Text className="text-[12px] text-muted-foreground">{label}</Text>
      <Text
        className={`flex-1 text-right text-[12px] ${
          danger ? "text-destructive" : "text-foreground"
        } ${mono ? "font-mono" : ""}`}
        numberOfLines={3}
      >
        {value}
      </Text>
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
  const bgClass =
    variant === "primary"
      ? "bg-primary"
      : variant === "destructive"
        ? "border border-destructive/40 bg-destructive/10"
        : "border border-border bg-card";
  const textClass =
    variant === "primary"
      ? "text-primary-foreground"
      : variant === "destructive"
        ? "text-destructive"
        : "text-foreground";
  const iconColor =
    variant === "primary"
      ? colors.primaryForeground
      : variant === "destructive"
        ? colors.destructive
        : colors.foreground;

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      className={`min-h-11 min-w-22 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl px-3 py-2.5 ${bgClass} active:opacity-70 disabled:opacity-50`}
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

function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
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
