import { i18n } from "@lingui/core";

/**
 * 格式化工具函数
 */

/** 格式化文件大小 */
/** 计算传输进度百分比（0-100 取整） */
export function calcPercent(transferred: number, total: number): number {
  return total > 0 ? Math.round((transferred / total) * 100) : 0;
}

/**
 * 格式化连接延迟。0ms 是取整后的占位值（<1ms 直连），看起来像 bug，
 * 故 ≤0 返回 null，由调用方决定只显示连接类型。
 */
export function formatLatency(ms: number | null | undefined): string | null {
  return ms != null && ms > 0 ? `${ms}ms` : null;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** 格式化传输速度（null 表示尚未估算出，显示 "—"） */
export function formatSpeed(bytesPerSec: number | null): string {
  if (bytesPerSec == null) return "—";
  return `${formatFileSize(bytesPerSec)}/s`;
}

/** 格式化剩余时间（秒） */
export function formatDuration(seconds: number): string {
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

/** 格式化分:秒倒计时 (如 "4:30")。只适合分钟量级——小时级请用 [`formatTimeLeft`]。 */
export function formatCountdown(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/**
 * 剩余时长的人类文案（邀请有效期用）。
 *
 * 邀请 TTL 从 5 分钟放宽到 24 小时后（openspec: invite-persistence），`m:ss` 会渲染成
 * 「1439:59」这种没人读得懂的东西，所以按量级切粒度：小时级只说小时+分钟，分钟级说分钟，
 * 最后一分钟才精确到秒（那时用户确实在盯着它）。
 *
 * **只返回时长本身，不带方向词**（「23 小时」而非「23 小时后」）：方向词归调用点的
 * `<Trans>`（「将在 {0} 后过期」/「Expires in {0}」），否则两边都说一次就成了
 * 「将在 23 小时后 后过期」。所以这里用 `Intl.NumberFormat` 的 unit 样式，
 * 而不是自带方向的 `Intl.RelativeTimeFormat`。
 *
 * **本地化**：跟随 Lingui 的当前 locale。早期版本返回硬编码中文，导致英文 UI 出现
 * 「Expires in 23 小时 59 分」这种混排 —— 这个函数的产物**一定**落在 `<Trans>` 的插值位，
 * 所以它自己必须是本地化的。
 *
 * 过期返回空字符串：调用方应当先判断过期并给自己的文案（见 `-sent-invites-section.tsx`），
 * 「已过期」是一句需要翻译的 UI 文案，不该由格式化函数硬编码。
 */
export function formatTimeLeft(seconds: number): string {
  if (seconds <= 0) return "";
  const unit = (value: number, unit: "hour" | "minute" | "second") =>
    new Intl.NumberFormat(i18n.locale || "zh", {
      style: "unit",
      unit,
      unitDisplay: "long",
    }).format(value);

  if (seconds < 60) return unit(seconds, "second");
  const totalMinutes = Math.round(seconds / 60);
  if (totalMinutes < 60) return unit(totalMinutes, "minute");
  return unit(Math.round(totalMinutes / 60), "hour");
}

/** 格式化相对时间 */
export function formatRelativeTime(date: Date | number): string {
  const now = Date.now();
  const ts = typeof date === "number" ? date : date.getTime();
  const diff = Math.floor((now - ts) / 1000);

  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}
