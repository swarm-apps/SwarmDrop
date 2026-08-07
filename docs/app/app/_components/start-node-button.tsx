"use client";

// 「启动节点」按钮。两个调用点：节点状态弹窗的动作区，以及各页「节点没在跑」空态里的出口。
//
// ## 在途状态读 store，不只读自己那次点击
//
// `action.pending` 只知道「**我**发起的那次还没回来」。但节点也可能是别处启动的——应用挂载时
// 的自动启动、另一个渲染点上的同一颗按钮。只看 `pending` 的话，那些情况下这颗按钮是亮着的
// 「启动节点」，点下去只会排进 lifecycle 的队列白跑一趟。
//
// 真源是 `status === "starting"`，`action` 只负责接住本次点击的失败。
//
// **不渲染自己的错误**：`startNodeRuntime` 失败时会把错误落进 store 的全局错误域，
// 每页顶部的 `WebErrorView` 已经在说同一件事，这里再来一份就是同一句话说两遍。
// （停止走的是另一条路——它不写全局错误域，所以那颗按钮自己接，见 node-status-dialog。）

import { Trans } from "@lingui/react/macro";
import { Power } from "lucide-react";
import { Button } from "@/components/ui/button";
import { startNodeRuntime } from "../_lib/node-lifecycle";
import { useWebNode } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";

export function StartNodeButton({
  /**
   * 调用点自己给 testid。**不写死一个共享值**：本组件同屏可能有两份（弹窗里一份、它背后的
   * 页面空态里一份），共用同一个 testid 会让选择器同时命中两个——而那两个的可见性还不一样，
   * e2e 拿到哪一个取决于 DOM 顺序。实测踩到过：采样脚本以为在点弹窗里那颗，其实点的是空态那颗。
   */
  testId,
  className = "",
  size = "default",
}: {
  testId: string;
  className?: string;
  size?: "sm" | "default";
}) {
  // 关停在途时也压住：那一拍过后状态会转 idle，届时按钮自然可用。
  const busy = useWebNode((s) => s.status === "starting" || s.status === "closing");
  const action = useAsyncAction();
  const pending = busy || action.pending;

  return (
    <Button
      size={size}
      disabled={pending}
      onClick={() => action.run(startNodeRuntime)}
      data-testid={testId}
      className={`gap-1.5 ${className}`}
    >
      <Power className="size-4" aria-hidden />
      {pending ? <Trans>启动中…</Trans> : <Trans>启动节点</Trans>}
    </Button>
  );
}
