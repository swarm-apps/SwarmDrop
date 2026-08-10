/**
 * 传输 UI 共享工具：方向图标、状态徽章、格式化函数、ProgressBar、状态判定。
 *
 * 与桌面端 `/Volumes/yexiyue/SwarmDrop/src/routes/_app/transfer/-shared.tsx`
 * 对齐，但用 RN + NativeWind 写法。
 */

import { Trans } from "@lingui/react/macro";
import { formatTransferRate } from "@swarmdrop/shared-view";
import { ArrowDownToLine, ArrowUpFromLine } from "lucide-react-native";
import type { ReactNode } from "react";
import { View } from "react-native";
import {
  type MobileFailureCode,
  MobileResumeRejectReason,
  MobileSuspendedReason,
  MobileTerminalReason,
  MobileTransferDirection,
  type MobileTransferProjection,
} from "react-native-swarmdrop-core";
import { Text } from "@/components/ui/text";
import {
  isProjectionRecoverable,
  type ProjectionStatus,
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
        <ArrowUpFromLine size={16} color={iconColor} strokeWidth={2.25} />
      ) : (
        <ArrowDownToLine size={16} color={iconColor} strokeWidth={2.25} />
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
  // 与「已取消 / 已拒绝」同为中性：没传成，但没出错。
  expired: {
    key: "expired",
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
      <Text className={cn("text-[12px] font-medium", meta.text)}>
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
    case "expired":
      return <Trans>未及时处理</Trans>;
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

/**
 * 进度所处的阶段。**颜色是它们之间唯一的视觉差别，别再共用一种**：
 *
 * - `transfer` —— 数据真的在网上跑，品牌青绿；
 * - `local` —— 纯本机阶段（准备 / 校验 / 打包），中性灰，还没上网；
 * - `paused` —— 传输已开始但停住了，琥珀警示色。
 *
 * 事实源是根目录 `DESIGN.md` 的 `Transfer Progress Contract`——三端共用那一份判据，
 * 只有机制各不相同（桌面 `src/components/ui/progress.tsx` 因轨道带品牌底纹而查表存
 * 轨道+填充成对，这里与 Web 的轨道本就中性，只需存填充）。
 *
 * `transfer`/`local` 与 `paused` 是**正交**的两组：前者分「本机准备 vs 真在传」，
 * 后者分「在传 vs 停住了」。本原语两组都画，故三项并列。
 *
 * `paused` 三端已统一（2026-08-10）：此前桌面 `packages/file-browser` 不换填充色、
 * 只用一个中性灰的 `<Pause />`，而这边是琥珀——同一状态一个「不显眼的灰」一个
 * 「需要注意的黄」。现在两边都是琥珀填充 + `CirclePause`。
 */
type ProgressTone = "transfer" | "local" | "paused";

/**
 * 填充色查表。
 *
 * **查表而不是让调用点传类名**：加一种阶段会在这里编译期报缺项，而不是变成某个页面
 * 忘了改的颜色——这条是 `Transfer Progress Contract` 明写的要求。上一版是
 * `fillClass?: string`，于是「哪些阶段存在」散在各调用点无从检查；换成查表时才发现
 * 除了 prepare 还有 file-row / file-card 两个调用点在传第三种颜色。
 *
 * 灰档用满不透明度而不是 `/60`：后者与轨道 `bg-muted` 在亮色下只有 2.19:1，
 * 够不上 WCAG 2.2 SC 1.4.11 对非文本组件的 3:1（满不透明度是 4.28:1 / 暗色 5.64:1）。
 */
const TONE_FILL: Record<ProgressTone, string> = {
  transfer: "bg-primary",
  local: "bg-muted-foreground",
  // `warning-ink` 不是 `warning`：琥珀原色填充与 `bg-muted` 轨道在**亮色**下只有
  // 1.95:1（暗色 6.83:1 没问题，只看暗色会以为没事）。ink 变体是 4.60:1 / 8.64:1。
  paused: "bg-warning-ink",
};

interface ProgressBarProps {
  percent: number;
  /** Tailwind height token，默认 h-2 */
  heightClass?: string;
  /** 进度所处阶段，决定填充色。默认 `transfer`。 */
  tone?: ProgressTone;
}

export function ProgressBar({
  percent,
  heightClass = "h-2",
  tone = "transfer",
}: ProgressBarProps) {
  const w = Math.min(100, Math.max(0, percent));
  return (
    <View className={cn("overflow-hidden rounded-full bg-muted", heightClass)}>
      <View
        className={cn("h-full", TONE_FILL[tone])}
        style={{ width: `${w}%` }}
      />
    </View>
  );
}

/* ─── 格式化函数 ─── */

/**
 * 字节量与百分比已收口到 `@swarmdrop/shared-view`（三端同一份取整规则）。
 * `formatBytes` 是移动端沿用的名字，保留以免全量改调用点。
 */
export {
  calcPercent,
  formatFileSize as formatBytes,
} from "@swarmdrop/shared-view";

/**
 * 速率：算不出来时显示 "—"。
 *
 * 共享的 [`formatTransferRate`] 返回 `null` 而非占位串——占位是一句要翻译的文案，
 * 三端各有各的说法，不该烤进格式化函数。这里补上移动端的破折号。
 */
export function formatSpeed(bytesPerSec: number | bigint | null): string {
  return formatTransferRate(bytesPerSec) ?? "—";
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
 * 会话失败判别码 → 文案。
 *
 * **这里曾经是 9 条英文关键词正则。** 它匹配的输入是
 * `format!("文件最终化失败: {name} (file_id={id}): {e}")` —— **文件名就拼在里面**，
 * 于是一个叫 `Q3-cancel.xlsx` 的文件校验失败会命中 `/(cancel|abort)/`，
 * 用户看到「传输已取消」：一次数据损坏被说成他自己的操作。确定性复现。
 *
 * 判别码把「是什么失败」和「怎么措辞」分开之后，猜测这件事根本不存在了。
 * `Legacy` 是判别码引入之前落库的自由文本，原样展示。
 */
export function failureCodeLabel(
  failure: MobileFailureCode | null | undefined,
): ReactNode {
  if (!failure) return null;
  switch (failure.tag) {
    case "SessionExpired":
      return (
        <Trans>超过 {failure.inner.retentionDays} 天未恢复，已自动清理</Trans>
      );
    case "ResumeRejected":
      return resumeRejectLabel(failure.inner.reason);
    case "OfferFailed":
      return <Trans>发送请求没能送达对方，请确认对方在线后重试</Trans>;
    case "PeerProtocolUnsupported":
      return (
        <Trans>
          对方的 SwarmDrop 版本太旧，无法接收这次传输，请让对方升级后重试
        </Trans>
      );
    case "Legacy":
      return failure.inner.message;
  }
}

function resumeRejectLabel(reason: MobileResumeRejectReason): ReactNode {
  switch (reason) {
    case MobileResumeRejectReason.Cancelled:
      return <Trans>对方已取消这次传输，无法继续</Trans>;
    case MobileResumeRejectReason.FatalError:
      return <Trans>对方那边出错了，无法继续</Trans>;
    case MobileResumeRejectReason.SourceModified:
      return <Trans>源文件已变更，无法继续，请重新发送</Trans>;
    case MobileResumeRejectReason.CheckpointInvalid:
      return <Trans>续传进度已失效，请重新发送</Trans>;
    case MobileResumeRejectReason.PeerUnavailable:
      return <Trans>对方暂时不可用，请稍后再试</Trans>;
    case MobileResumeRejectReason.SessionNotFound:
      return <Trans>对方已经没有这次传输的记录了</Trans>;
  }
}

export function LocalizedError({
  failure,
}: {
  failure: MobileFailureCode | null | undefined;
}) {
  const label = failureCodeLabel(failure);
  if (!label) return null;
  return <Text>{label}</Text>;
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
  if (projection.terminalReason === MobileTerminalReason.Expired) {
    return <Trans>请求超时未处理，让对方重发一次</Trans>;
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
  // 「可点恢复」的窄谓词 = phase===Suspended && recoverable(与桌面 canResumeProjection、
  // 本仓分组谓词 isProjectionRecoverable 同一事实来源)。core 的 recoverable 是「非终态、
  // 原则上可续传」的宽标志(offered/waiting/active 都为 true),不能单独当 UI 判据 —— 否则
  // 发送方等待对方接受(WaitingAccept)时会误显「恢复」。
  return isProjectionRecoverable(projection);
}

export function canResend(projection: MobileTransferProjection): boolean {
  return projection.direction === MobileTransferDirection.Send;
}
