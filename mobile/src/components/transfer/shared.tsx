/**
 * 传输 UI 共享工具：方向图标、状态徽章、格式化函数、ProgressBar、状态判定。
 *
 * 与桌面端 `/Volumes/yexiyue/SwarmDrop/src/routes/_app/transfer/-shared.tsx`
 * 对齐，但用 RN + NativeWind 写法。
 */

import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";
import {
  calcPercent,
  formatEta,
  formatFileSize,
  formatTransferRate,
  type UsableRates,
  usableRates,
} from "@swarmdrop/shared-view";
import { ArrowDownToLine, ArrowUpFromLine } from "lucide-react-native";
import type { ReactElement, ReactNode } from "react";
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
import type { ProgressFrame, PublishingFile } from "@/stores/transfer-store";

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
export { calcPercent, formatFileSize as formatBytes };

/**
 * 速率：算不出来时显示 "—"。
 *
 * 共享的 [`formatTransferRate`] 返回 `null` 而非占位串——占位是一句要翻译的文案，
 * 三端各有各的说法，不该烤进格式化函数。这里补上移动端的破折号。
 */
export function formatSpeed(bytesPerSec: number | bigint | null): string {
  return formatTransferRate(bytesPerSec) ?? "—";
}

/**
 * 一帧进度里**还能拿出来给人看**的两个数。
 *
 * **速度与剩余时间一起判、一起过期**：两者同源于后端的同一个滑窗，分开判的后果是同一行
 * 里一个诚实一个撒谎 —— ETA 已退成「计算中」，旁边还写着 `12.4 MB/s`，比两个都冻住更像
 * bug。判据（保鲜期）在 `@swarmdrop/shared-view`，三端同一份；「陈旧那一刻谁来触发重算」
 * 在 `transfer-store` 的 `ageProgressFrame`。
 *
 * 已传字节 / 百分比**不在此列**：它们是累计量，作废会让进度条倒退。
 */
function frameRates(frame: ProgressFrame | undefined): UsableRates {
  return usableRates(frame, frame?.receivedAt, Date.now());
}

/**
 * 活跃进度的第 4 个信息位「剩余时间」。
 *
 * 「12.4 MB/s」逼用户自己做除法才能得到他唯一关心的答案，所以速度是四位里最没用的一位；
 * 判据写在根目录 `DESIGN.md` 的 `Transfer Progress Contract`（含各表面的折叠规则：
 * 次要表面只放得下一个时，**留 ETA、丢速度**）。
 *
 * **算不出来时显示占位而不是让这一格消失**——消失读起来像布局 bug，而它恰恰在传输出问题
 * 时消失（后端停滞超过 3 秒滑窗就把 speed 归零、eta 变 null，那正是最该说点什么的时刻）。
 *
 * **收的是整帧而不是 `seconds`**：还要判这一帧是不是已经陈旧（[`frameRates`]）。后端会在
 * 下一帧诚实地把 eta 报成 null，但停滞时**根本没有下一帧**，不判时效就等于把一个早已
 * 不成立的「剩余 45s」一直显示到会话超时 —— ETA 不是在传输出问题时消失，而是在传输出
 * 问题时撒谎。
 *
 * **只在 `active` 的会话上渲染**：暂停的传输没有速度，在那里报一个「剩余」等于报一段
 * 根本不会发生的等待。
 */
function EtaLabel({ progress }: { progress: ProgressFrame | undefined }) {
  const eta = formatEta(frameRates(progress).eta);
  return eta === null ? <Trans>计算中</Trans> : <EtaValue eta={eta} />;
}

/**
 * 实时速率那一格。
 *
 * **必须与 [`EtaLabel`] 同源判时效**（都走 [`frameRates`]）：此前详情页直接渲染
 * `formatSpeed(Number(progress.speed))`，于是停滞时同一行显示「12.4 MB/s · 计算中」。
 * 算不出来时落到 [`formatSpeed`] 的 `—` 占位，不让这一格消失。
 */
export function SpeedLabel({
  progress,
}: {
  progress: ProgressFrame | undefined;
}) {
  return <>{formatSpeed(frameRates(progress).speed)}</>;
}

/**
 * ⚠️ **`props.eta` 不要解构。** Lingui 用简单标识符当占位名，成员表达式才产出 `{0}`；
 * 契约把 msgId 钉死成「剩余 {0}」以便四份 catalog 逐字对齐，解构会把它变成
 * 「剩余 {eta}」——一条谁也对不上的新串。
 */
function EtaValue(props: { eta: string }) {
  return <Trans>剩余 {props.eta}</Trans>;
}

/**
 * [`EtaLabel`] 的纯文本形态 —— 给 Android 前台通知这类拿不到 JSX 的调用点。
 *
 * 与组件版共用同一对 msgId，所以只有一处需要维护「算不出来说什么」这个决定。
 */
export function etaText(seconds: number | null | undefined): string {
  const eta = formatEta(seconds);
  return eta === null ? t`计算中` : remainingText({ eta });
}

/** 见 [`EtaValue`]：`props.eta` 同样不要解构，占位必须落成 `{0}`。 */
function remainingText(props: { eta: string }): string {
  return t`剩余 ${props.eta}`;
}

/**
 * 发布阶段（暂存 → 用户可见位置）的状态词，**点名正在保存哪个文件**。
 *
 * 三个表面共用这一份：契约允许次要表面用不点名的「正在保存…」，理由是「会话级横幅已经
 * 说清是哪个了」——**但移动端的活动页没有会话级横幅**，卡片就是唯一表面。放得下就带上。
 *
 * ⚠️ **`props.name` 不要解构**（同 [`EtaValue`]）：Lingui 用简单标识符当占位名，成员
 * 表达式才产出 `{0}`；契约把 msgId 钉死成「正在保存 {0}」以便四份 catalog 逐字对齐。
 */
function PublishingFileLabel(props: { name: string }) {
  return <Trans>正在保存 {props.name}</Trans>;
}

/**
 * 发布态文案槽 —— 详情页 / 活动卡 / 主屏行三处共用。
 *
 * 两条它必须自己扛住的性质：
 *
 * 1. **常驻挂载**（不发布时渲染空串，而不是整格消失）。Android 的 live region 播报的是
 *    「已经在树里的节点内容变了」；随内容一起挂上去的那种经常一声不响。空串同时也保住
 *    了这一格的宽度分配，进出发布态不会让同一行的其他格跳位。
 * 2. **`accessibilityLiveRegion="polite"` 只贴在这一格上**，不贴整行。整行还装着每秒都在
 *    变的字节数与 ETA，贴上去等于让读屏用户每秒被念一遍。
 *
 * 这条提示存在的**全部意义**就是解释一段静止（Android 的 SAF 全量拷贝，几十秒到几分钟），
 * 所以它恰恰是最该被念出来的一句。iOS 没有对等的 `announceForAccessibility` 调用是**故意**
 * 的：那一端的发布是同卷 rename，`started`/`finished` 背靠背，压根到不了
 * `PUBLISH_VISIBLE_AFTER_MS` 的揭示阈值 —— 写了也是一段永不执行的代码。
 */
function PublishingSlot({
  publishing,
  className,
}: {
  publishing: PublishingFile | undefined;
  className?: string;
}) {
  return (
    <Text
      accessibilityLiveRegion="polite"
      className={cn(
        "min-w-0 flex-1 text-[12px] text-muted-foreground",
        className,
      )}
      numberOfLines={1}
      testID={publishing ? "transfer-publishing-state" : undefined}
    >
      {publishing ? <PublishingFileLabel name={publishing.name} /> : ""}
    </Text>
  );
}

/**
 * 发布进度的百分比。**只在有字节可报时给**（Android 的 SAF 全量拷贝）：其余平台的发布
 * 是 O(1) 重命名、根本没有循环可上报，画一个永远停在 0% 的数字比不画更像卡死。
 */
function publishPercent(publishing: PublishingFile): number | null {
  if (publishing.publishedBytes <= 0) return null;
  return calcPercent(publishing.publishedBytes, publishing.totalBytes);
}

/* ─── 活跃进度页脚（详情页 / 活动卡 / 主屏行三处共用） ─── */

/** 页脚所在的表面。 */
export type ProgressReadoutSurface = "detail" | "card" | "row";

interface ReadoutSurfaceSpec {
  /** 进度条高度（Tailwind token）。 */
  bar: string;
  /** 条与页脚之间的间距。 */
  gap: string;
  /**
   * 发布期是否保留字节格。
   *
   * 次要表面（活动卡 / 主屏行）让位给「正在保存哪个文件」：发布期字节数恒等于
   * 「满 / 满」，一格没有信息量的数字会挤掉那一行唯一在说事的一句。详情页横向放得下，
   * 留着。
   */
  keepBytesWhilePublishing: boolean;
}

/**
 * 三处表面的密度与取舍。
 *
 * **查表而不是让调用点各传各的类名**：加一个表面会在这里编译期报缺项（同 [`TONE_FILL`]
 * 的理由）。三段页脚此前是逐字重复的 JSX，差别只有这张表里的三个字段 —— 于是「发布期
 * 该不该留字节格」这类决定散在三个文件里，谁也不知道另外两处是怎么写的。
 */
const READOUT_SURFACE: Record<ProgressReadoutSurface, ReadoutSurfaceSpec> = {
  detail: { bar: "h-2", gap: "gap-2", keepBytesWhilePublishing: true },
  card: { bar: "h-1.5", gap: "gap-1.5", keepBytesWhilePublishing: false },
  row: { bar: "h-1", gap: "gap-1.5", keepBytesWhilePublishing: false },
};

/** 页脚最右那格的三态，**互斥**。由 [`trailingReadoutOf`] 一处判定。 */
type TrailingReadout =
  | { kind: "publish-percent"; percent: number }
  | { kind: "eta" }
  | { kind: "none" };

/**
 * 最右那一格该说什么。
 *
 * 此前三处各写两个并排的条件表达式（`publishedPercent !== null ? … : null` 紧挨着
 * `!publishing && status === "transferring" ? … : null`），读者要自己推它们不会同时为真。
 * 三态一次判完：
 *
 * - 发布中且有字节可报 → 保存百分比（Android 的 SAF 拷贝，几十秒到几分钟）；
 * - 未发布且真的在传 → 剩余时间（一格只放得下一个时留 ETA、丢速度，见 `DESIGN.md`
 *   的折叠规则）；
 * - 其余（发布中但无字节可报 / 已暂停）→ 空。暂停的传输没有速度，报「剩余」等于报一段
 *   不会发生的等待。
 */
function trailingReadoutOf(
  publishing: PublishingFile | undefined,
  status: ProjectionStatus,
): TrailingReadout {
  if (publishing) {
    const percent = publishPercent(publishing);
    return percent === null
      ? { kind: "none" }
      : { kind: "publish-percent", percent };
  }
  return status === "transferring" ? { kind: "eta" } : { kind: "none" };
}

/**
 * 返回类型写死 `ReactElement | null`（不是 `ReactNode`）是**故意**的：`ReactNode` 含
 * `undefined`，switch 漏一档会静默返回 undefined；写成这样时漏档就是「函数缺少末尾
 * return」的编译错误。
 */
function TrailingReadout({
  publishing,
  progress,
  status,
}: {
  publishing: PublishingFile | undefined;
  progress: ProgressFrame | undefined;
  status: ProjectionStatus;
}): ReactElement | null {
  const readout = trailingReadoutOf(publishing, status);
  switch (readout.kind) {
    case "publish-percent":
      return (
        <Text className="text-[12px] tabular-nums text-muted-foreground">
          {readout.percent}%
        </Text>
      );
    case "eta":
      return (
        <Text className="text-[12px] tabular-nums text-muted-foreground">
          <EtaLabel progress={progress} />
        </Text>
      );
    case "none":
      return null;
  }
}

/**
 * 活跃进度的「条 + 其下那一行读数」—— 详情页 / 活动卡 / 主屏行三处共用。
 *
 * 三处此前是逐字重复的三段 JSX（条 + tone 三元 + 字节格 + 发布槽 + 保存百分比格 +
 * ETA 格），差别只有 [`READOUT_SURFACE`] 里那三个字段。
 *
 * **进度条的百分比恒是会话进度**，不因发布而换成本文件的保存进度 —— 那会让条倒退。
 * 发布是这套版式内部的一次替换（换 tone、换那一格说什么），不是另一套版式；判据在
 * 根目录 `DESIGN.md` 的 `Transfer Progress Contract`。
 */
export function TransferProgressReadout({
  surface,
  transferred,
  total,
  status,
  progress,
  publishing,
}: {
  surface: ProgressReadoutSurface;
  transferred: bigint;
  total: bigint;
  status: ProjectionStatus;
  progress: ProgressFrame | undefined;
  publishing: PublishingFile | undefined;
}) {
  const spec = READOUT_SURFACE[surface];
  const showBytes = !publishing || spec.keepBytesWhilePublishing;
  return (
    <View className={spec.gap}>
      {/* 发布期改用 local 灰：这一段没有字节在网上跑，继续用品牌青绿的满格条等于说
          「已经好了」。 */}
      <ProgressBar
        percent={calcPercent(transferred, total)}
        heightClass={spec.bar}
        tone={publishing ? "local" : "transfer"}
      />
      <View className="flex-row items-baseline justify-between gap-3">
        {showBytes ? (
          <Text className="text-[12px] text-muted-foreground">
            {`${formatFileSize(transferred)} / ${formatFileSize(total)}`}
          </Text>
        ) : null}
        {/* 右对齐只在左边还留着字节格时才有意义 —— 否则这一格就是行首。 */}
        <PublishingSlot
          publishing={publishing}
          className={cn(spec.keepBytesWhilePublishing && "text-right")}
        />
        <TrailingReadout
          publishing={publishing}
          progress={progress}
          status={status}
        />
      </View>
    </View>
  );
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
