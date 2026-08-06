"use client";

// WebError 统一收口：替代裸日志窗。kind 决定标题、message 用 mono 展示（可能含地址/技术细节）。
// `WebErrorCard` 是展示层，供全局横幅（本文件）与局部内联错误（connection-panel.tsx）共用。

import { useLingui } from "@lingui/react/macro";
import { useWebNode } from "../_lib/store";
import { WEB_ERROR_KIND_LABEL, type WebError } from "../_lib/view-types";

export function WebErrorCard({
  error,
  className = "",
  title,
}: {
  error: WebError;
  className?: string;
  /**
   * 覆盖标题。默认是 `error.kind` 的通用说法（「传输错误」），当一屏里可能同时出现好几张
   * 卡片时（如收件箱逐文件下载）那句话分不出是哪一件事失败了，调用方给一句具体的。
   */
  title?: string;
}) {
  const { t } = useLingui();
  return (
    <div
      role="alert"
      className={`rounded-lg border border-red-500/40 bg-red-50 px-4 py-3 text-sm dark:border-red-500/30 dark:bg-red-950/40 ${className}`}
    >
      <p className="font-medium text-red-900 dark:text-red-200">
        {title ?? t(WEB_ERROR_KIND_LABEL[error.kind])}
      </p>
      <p className="mt-1 font-mono text-xs break-all text-red-800/90 dark:text-red-200/80">
        {error.message}
      </p>
    </div>
  );
}

export function WebErrorView() {
  const error = useWebNode((s) => s.error);
  if (!error) return null;
  return <WebErrorCard error={error} />;
}
