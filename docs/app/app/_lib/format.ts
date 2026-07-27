// 格式化工具——与桌面 `src/lib/format.ts` 同名同语义（跨端一致的展示语言），
// 两端代码不共享（独立 workspace），但刻意保持函数名/取整规则一致。
// 另收纳跨面板共用的投影派生（排序键），避免各面板各写一份。

import type { TransferProjection } from "./view-types";

/**
 * 会话「结束时刻」：终态会话有 `finishedAt`，非终态回退到最后更新。
 * 收件箱排序与活动视图的耗时计算共用同一个定义。
 */
export function sessionEndedAt(projection: TransferProjection): number {
  return projection.finishedAt ?? projection.updatedAt;
}

/** 按最后更新时间倒序（不改原数组）。 */
export function sortByUpdatedDesc(items: TransferProjection[]): TransferProjection[] {
  return [...items].sort((a, b) => b.updatedAt - a.updatedAt);
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** 计算进度百分比（0-100 取整）。 */
export function calcPercent(done: number, total: number): number {
  return total > 0 ? Math.round((100 * done) / total) : 0;
}

/** 速率展示：`speed` 由内核按 bytes/s 下发。 */
export function formatTransferRate(bytesPerSecond: number | null | undefined): string {
  if (bytesPerSecond === null || bytesPerSecond === undefined || !Number.isFinite(bytesPerSecond)) {
    return "等待数据";
  }
  return `${formatFileSize(bytesPerSecond)}/s`;
}

/** 秒级时长展示，用于 ETA 与已用时间。 */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) {
    return "未知";
  }
  const rounded = Math.ceil(seconds);
  if (rounded < 60) return `${rounded} 秒`;
  const minutes = Math.floor(rounded / 60);
  const rest = rounded % 60;
  if (minutes < 60) return rest > 0 ? `${minutes} 分 ${rest} 秒` : `${minutes} 分`;
  const hours = Math.floor(minutes / 60);
  const minuteRest = minutes % 60;
  return minuteRest > 0 ? `${hours} 小时 ${minuteRest} 分` : `${hours} 小时`;
}
