"use client";

// 首屏节点面板：本机身份 + 状态。node id 是机器真值 → mono + tabular-nums（Mono Truth Rule）。
// 扁平卡片（shadow-xs），不堆装饰。

import { useWebNode } from "../_lib/store";
import { NodeStatusPill } from "./node-status-pill";

export function NodePanel() {
  const nodeId = useWebNode((s) => s.nodeId);
  const identityLocation = useWebNode((s) => s.identityLocation);

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-fd-foreground">本机节点</h2>
        <NodeStatusPill />
      </div>
      <dl className="mt-4 space-y-3">
        <div>
          <dt className="text-xs font-medium text-fd-muted-foreground">节点 ID</dt>
          <dd className="mt-1 font-mono text-xs tabular-nums break-all text-fd-foreground">
            {nodeId ?? "—"}
          </dd>
        </div>
        <div>
          <dt className="text-xs font-medium text-fd-muted-foreground">身份持久化</dt>
          <dd className="mt-1 text-sm text-fd-foreground">
            <span className="font-mono">{identityLocation}</span>{" "}
            <span className="text-fd-muted-foreground">· 刷新后保持不变</span>
          </dd>
        </div>
      </dl>
    </div>
  );
}
