/**
 * 传输 UI 共享工具：方向图标、状态徽章、格式化函数、ProgressBar、状态判定。
 *
 * 与桌面端 `/Volumes/yexiyue/SwarmDrop/src/routes/_app/transfer/-shared.tsx`
 * 对齐，但用 RN + NativeWind 写法。
 */

import { Trans } from "@lingui/react/macro";
import { Download, Send } from "lucide-react-native";
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

/**
 * 方向 chip:发送=纸飞机(呼应设备卡的发送按钮),接收=下载托盘(呼应收件箱)。
 * 底色浓度与 StatusBadge 的 /15 对齐 —— /10 在白底上若有若无,像没做完。
 */
export function DirectionIcon({ direction }: { direction: TransferDirection }) {
  const isSend = direction === "send";
  const colors = useThemeColors();
  const iconColor = isSend ? colors.primary : colors.success;
  return (
    <View
      className={cn(
        "size-10 items-center justify-center rounded-xl",
        isSend ? "bg-primary/15" : "bg-success/15",
      )}
    >
      {isSend ? (
        <Send size={16} color={iconColor} strokeWidth={2.25} />
      ) : (
        <Download size={16} color={iconColor} strokeWidth={2.25} />
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

/**
 * 状态色只用设计系统的 4 个语义 token(primary/success/warning/destructive)+ muted。
 * 与 `status-pill.tsx` 共用同一套色彩语汇,不引入 Tailwind 原生调色板(blue/yellow/orange)。
 * - 进行中 → primary(与进度条填充、实时 % 的蓝一致)
 * - 各类"待处理/暂停/可恢复中断" → warning(具体差异由 StatusLabel 文案承载,不靠色相区分)
 * - 完成 → success,失败 → destructive,已取消/已拒绝 → muted
 */
const STATUS_META: Record<string, StatusMeta> = {
  transferring: {
    key: "transferring",
    bg: "bg-primary/15",
    text: "text-primary-ink",
  },
  paused: { key: "paused", bg: "bg-warning/15", text: "text-warning-ink" },
  completed: {
    key: "completed",
    bg: "bg-success/15",
    text: "text-success-ink",
  },
  failed: {
    key: "failed",
    bg: "bg-destructive/15",
    text: "text-destructive-ink",
  },
  cancelled: {
    key: "cancelled",
    bg: "bg-muted",
    text: "text-muted-foreground",
  },
  waiting_accept: {
    key: "waiting_accept",
    bg: "bg-warning/15",
    text: "text-warning-ink",
  },
  offered: { key: "offered", bg: "bg-warning/15", text: "text-warning-ink" },
  interrupted: {
    key: "interrupted",
    bg: "bg-warning/15",
    text: "text-warning-ink",
  },
  peer_offline: {
    key: "peer_offline",
    bg: "bg-warning/15",
    text: "text-warning-ink",
  },
  app_restarted: {
    key: "app_restarted",
    bg: "bg-warning/15",
    text: "text-warning-ink",
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

/**
 * 把后端(Rust FfiError:io/network/transfer/database error…)抛出的技术错误串
 * 映射成"友好房东"口吻的中文。核心错误多为英文自由文本、无法穷举,所以用关键词
 * 启发式命中常见失败类别,未命中时降级到通用兜底 —— 绝不把原始英文直接甩给用户。
 */
export function friendlyTransferError(
  message: string | null | undefined,
): ReactNode {
  if (!message) return null;
  const m = message.toLowerCase();

  if (/reject/.test(m)) return <Trans>对方拒绝了这次传输</Trans>;
  if (/(cancel|abort)/.test(m)) return <Trans>传输已取消</Trans>;
  if (/(timeout|timed out|deadline)/.test(m))
    return <Trans>连接超时,请确认对方设备在线后重试</Trans>;
  if (/(offline|disconnect|not connected|peer.*(gone|left|closed))/.test(m))
    return <Trans>对方设备已离线,重新上线后可继续</Trans>;
  if (/(network|connection|connect|reset|broken pipe|unreachable|dial)/.test(m))
    return <Trans>网络连接中断,请确认两端在线后重试</Trans>;
  if (/(no space|disk full|enospc|quota)/.test(m))
    return <Trans>存储空间不足,清理后重试</Trans>;
  if (/(permission|denied|eacces|forbidden|unauthor)/.test(m))
    return <Trans>没有写入权限,请检查保存位置</Trans>;
  if (/(not found|enoent|no such file|missing file)/.test(m))
    return <Trans>找不到要传输的文件,可能已被移动或删除</Trans>;
  if (/(io error|read|write)/.test(m))
    return <Trans>读写文件时出错,请重试</Trans>;

  return <Trans>传输过程中出错了,请重试</Trans>;
}

export function LocalizedError({
  message,
}: {
  message: string | null | undefined;
}) {
  const friendly = friendlyTransferError(message);
  if (!friendly) return null;
  return <Text>{friendly}</Text>;
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
