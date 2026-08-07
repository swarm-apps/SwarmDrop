// 面板级 Suspense fallback：读 `useSearchParams()` 的面板在预渲染阶段拿不到 query，
// 静态导出下必须有边界（否则 `next build` 报 CSR bailout）。壳与真实面板同款，
// 切换时不跳版。

import type { ReactNode } from "react";

export function PanelFallback({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-xl border border-border bg-card p-6 text-xs text-muted-foreground shadow-xs">
      {children}
    </div>
  );
}
