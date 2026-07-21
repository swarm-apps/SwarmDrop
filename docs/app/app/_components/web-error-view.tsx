"use client";

// WebError 统一收口：替代裸日志窗。kind 决定标题、message 用 mono 展示（可能含地址/技术细节）。

import { useWebNode } from "../_lib/store";
import { WEB_ERROR_KIND_LABEL } from "../_lib/view-types";

export function WebErrorView() {
  const error = useWebNode((s) => s.error);
  if (!error) return null;

  return (
    <div
      role="alert"
      className="rounded-lg border border-red-500/40 bg-red-50 px-4 py-3 text-sm dark:border-red-500/30 dark:bg-red-950/40"
    >
      <p className="font-medium text-red-900 dark:text-red-200">{WEB_ERROR_KIND_LABEL[error.kind]}</p>
      <p className="mt-1 font-mono text-xs break-all text-red-800/90 dark:text-red-200/80">
        {error.message}
      </p>
    </div>
  );
}
