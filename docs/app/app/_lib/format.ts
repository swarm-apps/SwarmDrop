// 格式化工具——与桌面 `src/lib/format.ts` 同名同语义（跨端一致的展示语言），
// 两端代码不共享（独立 workspace），但刻意保持函数名/取整规则一致。

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
