/**
 * 会话详情页 —— 优先从 store.sessions（活跃）、再 store.dbHistory（历史快照），
 * 最后退到 native `getTransferSessionDetail` 兜底（deep link / 历史很久之前的会话）。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import { useLocalSearchParams, useRouter } from "expo-router";
import * as Sharing from "expo-sharing";
import { Pause, Play, Send, Trash2, X } from "lucide-react-native";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Alert,
  Pressable,
  ScrollView,
  View,
} from "react-native";
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
  LocalizedError,
  ProgressBar,
  StatusBadge,
  StatusLabel,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import type { TransferSession } from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { errorMessage } from "@/lib/utils";
import { useTransferStore } from "@/stores/transfer-store";

type View_ =
  | { kind: "active"; session: TransferSession }
  | { kind: "history"; item: MobileTransferHistoryItem }
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

  // native 兜底查询：只在 sessionId 变更且既不在 sessions 也不在 dbHistory 时
  // 拉一次；fetchedRef 防止 effect deps 变化导致重复请求。
  const [fallback, setFallback] = useState<View_ | null>(null);
  const fetchedForRef = useMemo(() => ({ id: null as string | null }), []);
  const [busy, setBusy] = useState<
    null | "pausing" | "cancelling" | "resuming" | "deleting" | "resending"
  >(null);

  useEffect(() => {
    if (!sessionId) return;
    if (session || historyMatch) {
      // 命中 store，无需 fallback
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
        if (!cancelled) setFallback({ kind: "history", item });
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

  const view: View_ = session
    ? { kind: "active", session }
    : historyMatch
      ? { kind: "history", item: historyMatch }
      : (fallback ?? { kind: "loading" });

  /* ─── 操作 handlers ─── */

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

  const onCancel = useCallback(async () => {
    if (!sessionId || busy) return;
    Alert.alert(t`取消传输`, t`确定要中止这次传输吗？`, [
      { text: t`继续`, style: "cancel" },
      {
        text: t`中止`,
        style: "destructive",
        onPress: async () => {
          setBusy("cancelling");
          try {
            await getMobileCore().cancelTransfer(sessionId);
          } catch (err) {
            toast.error(t`取消失败`, errorMessage(err));
          } finally {
            setBusy(null);
          }
        },
      },
    ]);
  }, [sessionId, busy, t]);

  const onResume = useCallback(async () => {
    if (!sessionId || busy) return;
    setBusy("resuming");
    try {
      const newId = await resumeHistoryItem(sessionId);
      // 如果 native 给的是新 sessionId（极少见），切换路由
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

  const onDelete = useCallback(async () => {
    if (!sessionId || busy) return;
    Alert.alert(t`删除这条记录`, t`仅从本机历史中删除，不影响对端。`, [
      { text: t`取消`, style: "cancel" },
      {
        text: t`删除`,
        style: "destructive",
        onPress: async () => {
          setBusy("deleting");
          try {
            await deleteHistoryItem(sessionId);
            router.back();
          } catch (err) {
            toast.error(t`删除失败`, errorMessage(err));
            setBusy(null);
          }
        },
      },
    ]);
  }, [sessionId, busy, deleteHistoryItem, router, t]);

  const onResend = useCallback(async () => {
    if (view.kind !== "history" || busy) return;
    const item = view.item;
    setBusy("resending");
    try {
      // 历史里没有 source_path（DB 不暴露），用 relativePath 作 sourceId 占位。
      // 实际能否 prepareSend 成功取决于原文件是否还在原位置。
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
  }, [view, busy, startSend, router, t]);

  const onShare = useCallback(
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
        // 用户取消分享不算错；其他错误降级到复制路径
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

  /* ─── 渲染 ─── */

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`传输详情`} />
      <ScrollView contentContainerClassName="gap-4 px-5 pt-2 pb-8">
        {view.kind === "loading" ? (
          <View className="items-center gap-3 py-20">
            <ActivityIndicator color={colors.mutedForeground} />
            <Text className="text-xs text-muted-foreground">
              <Trans>加载中</Trans>
            </Text>
          </View>
        ) : view.kind === "not-found" ? (
          <View className="items-center gap-2 py-20">
            <Text className="text-sm text-muted-foreground">
              <Trans>会话不存在或已结束</Trans>
            </Text>
          </View>
        ) : view.kind === "active" ? (
          <ActiveDetail
            session={view.session}
            onPause={onPause}
            onCancel={onCancel}
            busy={busy}
          />
        ) : (
          <HistoryDetail
            item={view.item}
            onShare={onShare}
            onCopyPath={onCopyPath}
            onResume={canResume(view.item) ? onResume : undefined}
            onResend={canResend(view.item) ? onResend : undefined}
            onDelete={onDelete}
            busy={busy}
          />
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

/* ─── 活跃 session 视图 ─── */

function ActiveDetail({
  session,
  onPause,
  onCancel,
  busy,
}: {
  session: TransferSession;
  onPause: () => void;
  onCancel: () => void;
  busy: string | null;
}) {
  const progress: MobileTransferProgress | null = session.progress;
  const total = Number(session.totalSize);
  const transferred = progress ? Number(progress.transferredBytes) : 0;
  const percent = calcPercent(transferred, total);

  const treeData = useMemo(
    () =>
      buildTreeDataFromOffer(
        session.files.map((f) => ({
          fileId: f.fileId,
          name: f.name,
          relativePath: f.relativePath || f.name,
          size: Number(f.size),
        })),
      ),
    [session.files],
  );

  return (
    <>
      <View className="flex-row items-center gap-3 rounded-xl border border-border bg-card p-4">
        <DirectionIcon direction={session.direction} />
        <View className="flex-1 gap-0.5">
          <Text
            className="text-[14px] font-semibold text-foreground"
            numberOfLines={1}
          >
            {session.peerName}
          </Text>
          <Text className="text-[11px] text-muted-foreground">
            {session.files.length} <Trans>个文件</Trans> ·{" "}
            {formatBytes(transferred)} / {formatBytes(total)}
          </Text>
        </View>
        <Text className="text-base font-bold text-foreground">{percent}%</Text>
      </View>

      <View className="gap-2">
        <View className="flex-row items-center justify-between">
          <Text className="text-xs text-muted-foreground">
            <Trans>状态</Trans>
          </Text>
          <StatusBadge status={session.status} />
        </View>
        <ProgressBar percent={percent} />
      </View>

      <FileTree
        mode="transfer"
        dataLoader={treeData.dataLoader}
        rootChildren={treeData.rootChildren}
        totalCount={session.files.length}
        totalSize={total}
        progress={progress}
      />

      <View className="flex-row gap-2 pt-2">
        <ActionButton
          icon={<Pause size={16} className="text-foreground" />}
          label={<Trans>暂停</Trans>}
          onPress={onPause}
          disabled={busy === "pausing"}
          variant="secondary"
          loading={busy === "pausing"}
        />
        <ActionButton
          icon={<X size={16} className="text-destructive" />}
          label={<Trans>取消</Trans>}
          onPress={onCancel}
          disabled={busy === "cancelling"}
          variant="destructive"
          loading={busy === "cancelling"}
        />
      </View>
    </>
  );
}

/* ─── 历史 item 视图 ─── */

function HistoryDetail({
  item,
  onShare,
  onCopyPath,
  onResume,
  onResend,
  onDelete,
  busy,
}: {
  item: MobileTransferHistoryItem;
  onShare: (uri: string, fileName: string) => void;
  onCopyPath: (uri: string) => void;
  onResume?: () => void;
  onResend?: () => void;
  onDelete: () => void;
  busy: string | null;
}) {
  // item.direction 是 native 端的 string，需要 narrow 到 TransferDirection
  const direction = item.direction === "send" ? "send" : "receive";
  const transferred = Number(item.transferredBytes);
  const total = Number(item.totalSize);
  const percent = calcPercent(transferred, total);
  const shareEnabled = canShareFile(item);
  const savePath = item.savePath ?? undefined;

  // 拼装单文件 URI：savePath 是会话级目录，file.relativePath / name 才是实际文件
  const fileUriOf = useCallback(
    (data: TreeNodeData): string | null => {
      if (!savePath) return null;
      const trimmed = savePath.replace(/\/$/, "");
      const sub = data.path.replace(/^\/+/, "");
      return `${trimmed}/${sub || data.name}`;
    },
    [savePath],
  );

  const treeData = useMemo(
    () =>
      buildTreeDataFromOffer(
        item.files.map((f) => ({
          fileId: f.fileId,
          name: f.name,
          relativePath: f.relativePath || f.name,
          size: Number(f.size),
        })),
      ),
    [item.files],
  );

  const handleFilePress = useCallback(
    (data: TreeNodeData) => {
      if (!shareEnabled) return;
      const uri = fileUriOf(data);
      if (uri) onShare(uri, data.name);
    },
    [shareEnabled, fileUriOf, onShare],
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
      <View className="flex-row items-center gap-3 rounded-xl border border-border bg-card p-4">
        <DirectionIcon direction={direction} />
        <View className="flex-1 gap-0.5">
          <Text
            className="text-[14px] font-semibold text-foreground"
            numberOfLines={1}
          >
            {item.peerName}
          </Text>
          <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
            <StatusLabel status={item.status} /> · {item.files.length}{" "}
            <Trans>个文件</Trans> · {formatBytes(total)}
          </Text>
        </View>
        <StatusBadge status={item.status} />
      </View>

      <View className="gap-3 rounded-xl border border-border bg-card p-4">
        <DetailRow
          label={<Trans>开始时间</Trans>}
          value={new Date(Number(item.startedAt)).toLocaleString()}
        />
        {item.finishedAt ? (
          <DetailRow
            label={<Trans>结束时间</Trans>}
            value={new Date(Number(item.finishedAt)).toLocaleString()}
          />
        ) : null}
        <DetailRow
          label={<Trans>已传 / 总量</Trans>}
          value={`${formatBytes(transferred)} / ${formatBytes(total)} (${percent}%)`}
        />
        <DetailRow label={<Trans>对端</Trans>} value={item.peerId} mono />
        {item.errorMessage ? (
          <DetailRow
            label={<Trans>错误原因</Trans>}
            value={<LocalizedError message={item.errorMessage} />}
            danger
          />
        ) : null}
        {item.savePath ? (
          <DetailRow
            label={<Trans>保存位置</Trans>}
            value={item.savePath}
            mono
          />
        ) : null}
      </View>

      {/* 文件树（mode=transfer，按状态着色；可点击分享 / 长按复制） */}
      <FileTree
        mode="transfer"
        dataLoader={treeData.dataLoader}
        rootChildren={treeData.rootChildren}
        totalCount={item.files.length}
        totalSize={total}
        onFilePress={shareEnabled ? handleFilePress : undefined}
        onFileLongPress={savePath ? handleFileLongPress : undefined}
      />

      {/* 操作按钮组 */}
      <View className="flex-row flex-wrap gap-2 pt-2">
        {onResume ? (
          <ActionButton
            icon={<Play size={16} className="text-primary-foreground" />}
            label={<Trans>恢复</Trans>}
            onPress={onResume}
            disabled={busy === "resuming"}
            variant="primary"
            loading={busy === "resuming"}
          />
        ) : null}
        {onResend ? (
          <ActionButton
            icon={<Send size={16} className="text-foreground" />}
            label={<Trans>重新发送</Trans>}
            onPress={onResend}
            disabled={busy === "resending"}
            variant="secondary"
            loading={busy === "resending"}
          />
        ) : null}
        <ActionButton
          icon={<Trash2 size={16} className="text-destructive" />}
          label={<Trans>删除</Trans>}
          onPress={onDelete}
          disabled={busy === "deleting"}
          variant="destructive"
          loading={busy === "deleting"}
        />
      </View>
    </>
  );
}

/* ─── 子组件 ─── */

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
  icon: React.ReactNode;
  label: React.ReactNode;
  onPress: () => void;
  disabled?: boolean;
  loading?: boolean;
  variant: "primary" | "secondary" | "destructive";
}

function ActionButton({
  icon,
  label,
  onPress,
  disabled,
  loading,
  variant,
}: ActionButtonProps) {
  const classes =
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

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      className={`min-h-11 min-w-22 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl px-3 py-2.5 ${classes} active:opacity-70 disabled:opacity-50`}
    >
      {loading ? <ActivityIndicator size="small" /> : icon}
      <Text className={`text-[13px] font-medium ${textClass}`}>{label}</Text>
    </Pressable>
  );
}
