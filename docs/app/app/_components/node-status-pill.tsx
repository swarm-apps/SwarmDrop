"use client";

// 节点状态徽章。状态点用语义色（green/amber/red）——属「状态诚实可见」的语义编码，
// 在 DESIGN 的 one-accent 规则之外，不算第二强调色。脉冲动效带 motion-reduce 降级。
//
// 图标侧栏（768–1023px）那一档宽度放不下文字，调用方传 `labelClassName="hidden lg:inline"`
// 把标签藏掉即可——不需要第二个组件。文字藏起来时读屏与悬停仍拿得到：
// `title` / `aria-label` 常驻，「节点是否在跑」不因窗口窄而消失（PRODUCT.md 原则 2）。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { useLingui } from "@lingui/react/macro";
import { useWebNode, type NodeStatus } from "../_lib/store";
import { StatusDot } from "./status-dot";

const STATUS_META: Record<NodeStatus, { label: MessageDescriptor; dot: string; pulse?: boolean }> = {
  idle: { label: msg`未启动`, dot: "bg-fd-muted-foreground" },
  starting: { label: msg`启动中`, dot: "bg-amber-500", pulse: true },
  running: { label: msg`运行中`, dot: "bg-emerald-500" },
  closing: { label: msg`关停中`, dot: "bg-amber-500", pulse: true },
  error: { label: msg`启动失败`, dot: "bg-red-500" },
};

export function NodeStatusPill({
  className = "",
  labelClassName = "",
}: {
  className?: string;
  labelClassName?: string;
}) {
  const { t } = useLingui();
  const status = useWebNode((s) => s.status);
  const meta = STATUS_META[status];
  const phase = t(meta.label);
  // 「节点」+ 状态词拼在一起是给读屏与 tooltip 的完整句；可见文本只留状态词（空间有限）。
  const label = t`节点${phase}`;

  return (
    <span
      role="status"
      title={label}
      aria-label={label}
      className={`inline-flex items-center gap-1.5 rounded-full border border-fd-border bg-fd-card px-2.5 py-1 text-xs font-medium text-fd-muted-foreground shadow-xs ${className}`}
    >
      <StatusDot colorClass={meta.dot} pulse={meta.pulse} />
      <span className={labelClassName}>{phase}</span>
    </span>
  );
}
