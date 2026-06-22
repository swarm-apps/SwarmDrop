/**
 * 格式化工具函数
 */

/** 格式化文件大小 */
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

/** 格式化分:秒倒计时 (如 "4:30") */
export function formatCountdown(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
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
