"use client";

// 顶栏节点状态徽章。状态点用语义色（green/amber/red）——属「状态诚实可见」的语义编码，
// 在 DESIGN 的 one-accent 规则之外，不算第二强调色。脉冲动效带 motion-reduce 降级。

import { useWebNode, type NodeStatus } from "../_lib/store";
import { StatusDot } from "./status-dot";

const STATUS_META: Record<NodeStatus, { label: string; dot: string; pulse?: boolean }> = {
  idle: { label: "未启动", dot: "bg-fd-muted-foreground" },
  starting: { label: "启动中", dot: "bg-amber-500", pulse: true },
  running: { label: "运行中", dot: "bg-emerald-500" },
  closing: { label: "关停中", dot: "bg-amber-500", pulse: true },
  error: { label: "启动失败", dot: "bg-red-500" },
};

export function NodeStatusPill() {
  const status = useWebNode((s) => s.status);
  const meta = STATUS_META[status];
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-fd-border bg-fd-card px-2.5 py-1 text-xs font-medium text-fd-muted-foreground shadow-xs">
      <StatusDot colorClass={meta.dot} pulse={meta.pulse} />
      {meta.label}
    </span>
  );
}
