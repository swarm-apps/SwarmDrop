/**
 * 传输 UI 共享工具：方向图标、状态徽章、格式化函数、ProgressBar、状态判定。
 *
 * 与桌面端 `/Volumes/yexiyue/SwarmDrop/src/routes/_app/transfer/-shared.tsx`
 * 对齐，但用 RN + NativeWind 写法。
 */

import { Trans } from "@lingui/react/macro";
import { ArrowDownLeft, ArrowUpRight } from "lucide-react-native";
import type { ReactNode } from "react";
import { View } from "react-native";
import {
  MobileSuspendedReason,
  MobileTerminalReason,
  MobileTransferDirection,
  type MobileTransferProjection,
} from "react-native-swarmdrop-core";
import { Text } from "@/components/ui/text";
import {
  type ProjectionStatus,
  projectionStatus,
  type TransferDirection,
} from "@/core/transfer-types";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

/* ─── 方向图标 ─── */

export function DirectionIcon({ direction }: { direction: TransferDirection }) {
  const isSend = direction === "send";
  const colors = useThemeColors();
  const iconColor = isSend ? colors.primary : colors.success;
  return (
    <View
      className={cn(
        "size-10 items-center justify-center rounded-xl",
        isSend ? "bg-primary/10" : "bg-success/10",
      )}
    >
      {isSend ? (
        <ArrowUpRight size={18} color={iconColor} strokeWidth={2.5} />
      ) : (
        <ArrowDownLeft size={18} color={iconColor} strokeWidth={2.5} />
      )}
    </View>
  );
}

/* ─── 状态徽章 ─── */

export type AnyStatus = ProjectionStatus;

interface StatusMeta {
  key: string;
  bg: string;
  text: string;
}

const STATUS_META: Record<string, StatusMeta> = {
  transferring: {
    key: "transferring",
    bg: "bg-blue-500/15",
    text: "text-blue-500",
  },
  paused: {
    key: "paused",
    bg: "bg-yellow-500/15",
    text: "text-yellow-600 dark:text-yellow-400",
  },
  completed: { key: "completed", bg: "bg-success/15", text: "text-success" },
  failed: { key: "failed", bg: "bg-destructive/15", text: "text-destructive" },
  cancelled: {
    key: "cancelled",
    bg: "bg-muted",
    text: "text-muted-foreground",
  },
  waiting_accept: {
    key: "waiting_accept",
    bg: "bg-yellow-500/15",
    text: "text-yellow-600 dark:text-yellow-400",
  },
  offered: {
    key: "offered",
    bg: "bg-yellow-500/15",
    text: "text-yellow-600 dark:text-yellow-400",
  },
  interrupted: {
    key: "interrupted",
    bg: "bg-orange-500/15",
    text: "text-orange-600 dark:text-orange-400",
  },
  peer_offline: {
    key: "peer_offline",
    bg: "bg-orange-500/15",
    text: "text-orange-600 dark:text-orange-400",
  },
  app_restarted: {
    key: "app_restarted",
    bg: "bg-orange-500/15",
    text: "text-orange-600 dark:text-orange-400",
  },
  rejected: {
    key: "rejected",
    bg: "bg-muted",
    text: "text-muted-foreground",
  },
};

const FALLBACK_META: StatusMeta = {
  key: "unknown",
  bg: "bg-muted",
  text: "text-muted-foreground",
};

function statusMetaOf(status: AnyStatus): StatusMeta {
  const key = statusKey(status);
  return STATUS_META[key] ?? FALLBACK_META;
}

export function statusKey(status: AnyStatus): string {
  return status;
}

export function StatusBadge({ status }: { status: AnyStatus }) {
  const meta = statusMetaOf(status);
  return (
    <View className={cn("rounded-full px-2 py-0.5", meta.bg)}>
      <Text className={cn("text-[11px] font-medium", meta.text)}>
        <StatusLabel status={status} />
      </Text>
    </View>
  );
}

export function StatusLabel({ status }: { status: AnyStatus }) {
  switch (statusKey(status)) {
    case "offered":
      return <Trans>待确认</Trans>;
    case "transferring":
      return <Trans>传输中</Trans>;
    case "paused":
      return <Trans>已暂停</Trans>;
    case "completed":
      return <Trans>已完成</Trans>;
    case "failed":
      return <Trans>失败</Trans>;
    case "cancelled":
      return <Trans>已取消</Trans>;
    case "rejected":
      return <Trans>已拒绝</Trans>;
    case "waiting_accept":
      return <Trans>等待响应</Trans>;
    case "interrupted":
      return <Trans>可恢复中断</Trans>;
    case "peer_offline":
      return <Trans>对端离线</Trans>;
    case "app_restarted":
      return <Trans>应用重启</Trans>;
    default:
      return <Trans>未知</Trans>;
  }
}

/* ─── 进度条（共享给详情页 / 发送准备页 / file-tree-item） ─── */

interface ProgressBarProps {
  percent: number;
  /** Tailwind height token，默认 h-2 */
  heightClass?: string;
  /** 填充色 className，默认 bg-primary */
  fillClass?: string;
}

export function ProgressBar({
  percent,
  heightClass = "h-2",
  fillClass = "bg-primary",
}: ProgressBarProps) {
  const w = Math.min(100, Math.max(0, percent));
  return (
    <View className={cn("overflow-hidden rounded-full bg-muted", heightClass)}>
      <View className={cn("h-full", fillClass)} style={{ width: `${w}%` }} />
    </View>
  );
}

/* ─── 格式化函数 ─── */

export function formatBytes(bytes: number | bigint): string {
  const n = typeof bytes === "bigint" ? Number(bytes) : bytes;
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** 速率：null / 非正数显示 "—"（与桌面 formatSpeed 行为一致）。 */
export function formatSpeed(bytesPerSec: number | bigint | null): string {
  if (bytesPerSec == null) return "—";
  const n = typeof bytesPerSec === "bigint" ? Number(bytesPerSec) : bytesPerSec;
  if (!Number.isFinite(n) || n <= 0) return "—";
  return `${formatBytes(n)}/s`;
}

export function calcPercent(
  transferred: number | bigint,
  total: number | bigint,
): number {
  const t = typeof total === "bigint" ? Number(total) : total;
  const tr =
    typeof transferred === "bigint" ? Number(transferred) : transferred;
  return t > 0 ? Math.min(100, Math.round((tr / t) * 100)) : 0;
}

/** 相对时间，不引入 dayjs/date-fns，保持依赖最小 */
export function formatRelativeTime(timestampMs: number | bigint): ReactNode {
  const ms =
    typeof timestampMs === "bigint" ? Number(timestampMs) : timestampMs;
  const diff = Date.now() - ms;
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;

  if (diff < minute) return <Trans>刚刚</Trans>;
  if (diff < hour) {
    const m = Math.floor(diff / minute);
    return <Trans>{m} 分钟前</Trans>;
  }
  if (diff < day) {
    const h = Math.floor(diff / hour);
    return <Trans>{h} 小时前</Trans>;
  }
  if (diff < 7 * day) {
    const d = Math.floor(diff / day);
    return <Trans>{d} 天前</Trans>;
  }
  const date = new Date(ms);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function pad(n: number) {
  return n.toString().padStart(2, "0");
}

/* ─── 错误/原因 i18n 映射 ─── */

export function LocalizedError({
  message,
}: {
  message: string | null | undefined;
}) {
  if (!message) return null;
  return <Text>{message}</Text>;
}

export function projectionReasonLabel(
  projection: MobileTransferProjection,
): ReactNode {
  if (projection.suspendedReason === MobileSuspendedReason.LocalPaused) {
    return <Trans>本机暂停</Trans>;
  }
  if (projection.suspendedReason === MobileSuspendedReason.RemotePaused) {
    return <Trans>对端暂停</Trans>;
  }
  if (projection.suspendedReason === MobileSuspendedReason.Interrupted) {
    return <Trans>网络中断</Trans>;
  }
  if (projection.suspendedReason === MobileSuspendedReason.PeerOffline) {
    return <Trans>对端离线</Trans>;
  }
  if (projection.suspendedReason === MobileSuspendedReason.AppRestarted) {
    return <Trans>应用重启后可恢复</Trans>;
  }
  if (projection.terminalReason === MobileTerminalReason.Rejected) {
    return <Trans>请求被拒绝</Trans>;
  }
  if (projection.terminalReason === MobileTerminalReason.Cancelled) {
    return <Trans>传输已取消</Trans>;
  }
  if (projection.terminalReason === MobileTerminalReason.FatalError) {
    return <Trans>传输失败</Trans>;
  }
  return null;
}

export function canShareFile(projection: MobileTransferProjection): boolean {
  return (
    projection.direction === MobileTransferDirection.Receive &&
    projection.terminalReason === MobileTerminalReason.Completed &&
    !!projection.saveLocation
  );
}

export function canResume(projection: MobileTransferProjection): boolean {
  return (
    projection.recoverable && projectionStatus(projection) !== "transferring"
  );
}

export function canResend(projection: MobileTransferProjection): boolean {
  return projection.direction === MobileTransferDirection.Send;
}
