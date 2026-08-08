"use client";

// 节点状态徽章 —— 常驻位那一格。状态点用语义色（green/amber/neutral），属「状态诚实可见」的
// 语义编码，在 DESIGN 的 one-accent 规则之外；脉冲动效带 motion-reduce 降级。
//
// 交互不在这里：它是 `node-status-dialog.tsx` 的触发器外观，也是那个弹窗自己标题下的状态行。
// 两处要长得一模一样，所以画法只有一份，而「点开会发生什么」由包着它的那一层决定。
//
// ## 「运行中」不等于「连得上」——这一格必须说后者
//
// 徽章此前只映射前端生命周期（idle / starting / running / closing / error），于是节点在跑、
// 全部引导节点都挂着的时候它照样是**绿的**。那正是 DESIGN.md 的 Node Status Contract 要禁的：
// 常驻位的色档必须来自 `summarizeNodeHealth`，它回答的是「别人现在能不能连到我」。
//
// 生命周期那几档没有被丢掉：节点没在跑时，「启动中 / 关停中 / 启动失败」比一句「未运行」
// 有用得多（用户刚点了启动，得知道它在动）。所以分两段——**没在跑看生命周期，跑起来看健康度**。
//
// ## 状态词在任何一档都不许消失
//
// 图标侧栏（768–1023px）那一档曾传 `labelClassName="hidden lg:inline"` 把标签藏掉，于是常驻
// 结论层退化成一个 36×36 圆里的**裸色点**——而契约的信息位 1 明写「状态点**和词**，光一个
// 色点不满足」，下一句又禁止因布局紧张丢掉信息位。`title` / `aria-label` 只覆盖悬停与读屏，
// 不算数：那两条是给拿不到视觉的人的补充，不是视觉的替代。
//
// 现在那一档改成**竖排**（点在上、词在下，同窄屏底部导航「图标 + 11px 标签」的排法），
// 词照常显示。64px 的栏宽装得下：`md:text-[10px] + tracking-tight` 下最长的英文串
// "Connecting" 约 48px，容器让出的可用宽度 50px；再长的（"Failed to start"）在空格处换行。
// 形态类由调用方给（见 `_components/app-nav.tsx` 的 `AppSidebar`）——那里才知道栏有多宽。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { useLingui } from "@lingui/react/macro";
import { NODE_HEALTH_WORD, TONE_DOT } from "../_lib/network-view";
import { useWebNode, type NodeStatus } from "../_lib/store";
import { useNodeHealth } from "../_lib/use-network-status";
import { StatusDot } from "./status-dot";

/** 节点**没在跑**时的几档。跑起来之后由健康度接管，所以这里没有 `running`。 */
const LIFECYCLE_META: Record<
  Exclude<NodeStatus, "running">,
  { label: MessageDescriptor; dot: string; pulse: boolean }
> = {
  idle: { label: msg`未启动`, dot: "bg-muted-foreground", pulse: false },
  starting: { label: msg`启动中`, dot: "bg-warning", pulse: true },
  closing: { label: msg`关停中`, dot: "bg-warning", pulse: true },
  error: { label: msg`启动失败`, dot: "bg-destructive", pulse: false },
};

export interface NodeStatusPresentation {
  /** 状态**词**（契约结论层信息位 1 要的是「点 + 词」，光一个色点不满足）。 */
  label: string;
  dot: string;
  pulse: boolean;
}

/**
 * 徽章要画的那三样。
 *
 * 供徽章本体与它的可访问名共用——两者若各算一遍，`title` 说的和眼睛看到的迟早会不一样。
 */
export function useNodeStatusPresentation(): NodeStatusPresentation {
  const { t } = useLingui();
  const status = useWebNode((s) => s.status);
  const health = useNodeHealth();

  if (status !== "running") {
    const meta = LIFECYCLE_META[status];
    return { label: t(meta.label), dot: meta.dot, pulse: meta.pulse };
  }
  return {
    label: t(NODE_HEALTH_WORD[health.level]),
    dot: TONE_DOT[health.tone],
    // 「正在连接网络…」是过程态，脉冲说的就是「还在动，别急」。
    pulse: health.level === "starting",
  };
}

export function NodeStatusPill({
  className = "",
  labelClassName = "",
}: {
  className?: string;
  labelClassName?: string;
}) {
  const { label, dot, pulse } = useNodeStatusPresentation();

  return (
    <span
      // 文字用 `text-foreground` 而不是 muted：状态词是**事实**不是标签，它也必须比同处
      // 侧栏底部的三枚灰工具图标重一档，否则「本页唯一诚实汇报运行时的地方」和「调一下环境」
      // 在视觉上同级。muted 在 12px + `bg-card` 上本来也刚压着 AA 线。
      className={`inline-flex items-center gap-1.5 rounded-full border border-border bg-card px-2.5 py-1 text-xs font-medium text-foreground shadow-xs ${className}`}
    >
      <StatusDot colorClass={dot} pulse={pulse} />
      <span className={labelClassName}>{label}</span>
    </span>
  );
}
