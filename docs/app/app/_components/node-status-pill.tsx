"use client";

// 节点状态徽章 —— **纯展示**。状态点用语义色（green/amber/red），属「状态诚实可见」的语义
// 编码，在 DESIGN 的 one-accent 规则之外；脉冲动效带 motion-reduce 降级。
//
// 交互不在这里：它是 `node-status-dialog.tsx` 的触发器外观，也是那个弹窗自己标题下的状态行。
// 两处要长得一模一样，所以画法只有一份，而「点开会发生什么」由包着它的那一层决定。
//
// 图标侧栏（768–1023px）那一档宽度放不下文字，调用方传 `labelClassName="hidden lg:inline"`
// 把标签藏掉即可——不需要第二个组件。文字藏起来时读屏与悬停仍拿得到：`title` / `aria-label`
// 由外层的触发按钮常驻，「节点是否在跑」不因窗口窄而消失（PRODUCT.md 原则 2）。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { useLingui } from "@lingui/react/macro";
import type { NodeStatus } from "../_lib/store";
import { StatusDot } from "./status-dot";

const STATUS_META: Record<NodeStatus, { label: MessageDescriptor; dot: string; pulse?: boolean }> = {
  idle: { label: msg`未启动`, dot: "bg-muted-foreground" },
  starting: { label: msg`启动中`, dot: "bg-amber-500", pulse: true },
  running: { label: msg`运行中`, dot: "bg-emerald-500" },
  closing: { label: msg`关停中`, dot: "bg-amber-500", pulse: true },
  error: { label: msg`启动失败`, dot: "bg-destructive" },
};

/** 状态词。可见文本只留它（空间有限），完整句由调用方拼给读屏。 */
export function useNodeStatusLabel(status: NodeStatus): string {
  const { t } = useLingui();
  return t(STATUS_META[status].label);
}

export function NodeStatusPill({
  status,
  className = "",
  labelClassName = "",
}: {
  status: NodeStatus;
  className?: string;
  labelClassName?: string;
}) {
  const meta = STATUS_META[status];
  const label = useNodeStatusLabel(status);

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border border-border bg-card px-2.5 py-1 text-xs font-medium text-muted-foreground shadow-xs ${className}`}
    >
      <StatusDot colorClass={meta.dot} pulse={meta.pulse} />
      <span className={labelClassName}>{label}</span>
    </span>
  );
}
