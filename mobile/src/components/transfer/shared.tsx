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
  MobileSessionStatus,
  type MobileTransferHistoryItem,
} from "react-native-swarmdrop-core";
import { Text } from "@/components/ui/text";
import {
  type ActiveStatus,
  ERROR_APP_INTERRUPTED,
  type TransferDirection,
} from "@/core/transfer-types";
import { cn } from "@/lib/utils";

/* ─── 方向图标 ─── */

export function DirectionIcon({ direction }: { direction: TransferDirection }) {
  const isSend = direction === "send";
  return (
    <View
      className={cn(
        "size-10 items-center justify-center rounded-xl",
        isSend ? "bg-primary/10" : "bg-success/10",
      )}
    >
      {isSend ? (
        <ArrowUpRight size={18} className="text-primary" strokeWidth={2.5} />
      ) : (
        <ArrowDownLeft size={18} className="text-success" strokeWidth={2.5} />
      )}
    </View>
  );
}

/* ─── 状态徽章 ─── */

export type AnyStatus = MobileSessionStatus | ActiveStatus;

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

/** 把 native enum 或 active 字符串统一归约成一个 string key */
export function statusKey(status: AnyStatus): string {
  if (typeof status === "string") return status;
  switch (status) {
    case MobileSessionStatus.Transferring:
      return "transferring";
    case MobileSessionStatus.Paused:
      return "paused";
    case MobileSessionStatus.Completed:
      return "completed";
    case MobileSessionStatus.Failed:
      return "failed";
    case MobileSessionStatus.Cancelled:
      return "cancelled";
    default:
      return "unknown";
  }
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
    case "waiting_accept":
      return <Trans>等待响应</Trans>;
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

/* ─── 错误消息 i18n 映射 ─── */

export function LocalizedError({
  message,
}: {
  message: string | null | undefined;
}) {
  if (!message) return null;
  if (message === ERROR_APP_INTERRUPTED) {
    return <Trans>上次未完成</Trans>;
  }
  return <Text>{message}</Text>;
}

/* ─── 历史 item 判定辅助 ─── */

export function canShareFile(item: MobileTransferHistoryItem): boolean {
  return (
    item.direction === "receive" &&
    item.status === MobileSessionStatus.Completed &&
    !!item.savePath
  );
}

export function canResume(item: MobileTransferHistoryItem): boolean {
  return (
    item.status === MobileSessionStatus.Paused ||
    item.status === MobileSessionStatus.Failed
  );
}

export function canResend(item: MobileTransferHistoryItem): boolean {
  return item.direction === "send";
}
