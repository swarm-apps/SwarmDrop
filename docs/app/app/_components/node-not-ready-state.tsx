"use client";

// 「节点没在跑，所以这里暂时没有内容」的统一说法。
//
// ## 为什么必须按状态分说
//
// 此前各页一律写「正在启动节点…」，判据是 `status !== "running"`。节点能停之后这句话就会
// 说谎：用户刚亲手停掉节点，界面却告诉他正在启动——而他等不到任何结果。
//
// 四种状态是四件不同的事，用户的下一步也不同：
//
//   starting  等着就行
//   closing   等着就行（片刻后转 idle）
//   idle      要他动手：这里给一颗启动按钮
//   error     要他动手：同上，具体原因由页面顶部的全局错误条讲
//
// ## 空态自带出口，而不是指路
//
// 「去点左下角那枚徽章」这类方位指代在窄屏是错的（那时它在顶栏），而且多一次寻找。
// `CenteredEmptyState` 的 `action` 位就是为此存在的——启动节点这件事在哪儿触发都是
// 同一个 `startNodeRuntime`，直接摆一颗按钮。

import { Trans } from "@lingui/react/macro";
import { Loader2, Power } from "lucide-react";
import type { ReactNode } from "react";
import { useWebNode } from "../_lib/store";
import { CenteredEmptyState } from "./empty-state";
import { StartNodeButton } from "./start-node-button";

/**
 * @param description 「节点起来后**这一页**会有什么」——各页自己说，因为答案不同
 *                    （设备清单 / 传输会话 / 可发送的目标）。
 * @param fill        透传给 [`CenteredEmptyState`]：满高居中（详情栏）还是收到内容高度
 *                    （面板里的一段）。判据与那里一致，不要在这里另立一套。
 */
export function NodeNotReadyState({
  description,
  fill = true,
}: {
  description: ReactNode;
  fill?: boolean;
}) {
  const status = useWebNode((s) => s.status);

  if (status === "starting" || status === "closing") {
    return (
      <CenteredEmptyState
        icon={Loader2}
        // 唯一一处图标本身要转的空态：它说的是「正在发生」，静止的转圈图标是自相矛盾的。
        iconClassName="animate-spin"
        fill={fill}
        title={
          status === "starting" ? <Trans>正在启动节点…</Trans> : <Trans>正在停止节点…</Trans>
        }
        description={description}
      />
    );
  }

  return (
    <CenteredEmptyState
      icon={Power}
      fill={fill}
      title={
        status === "error" ? <Trans>节点没能启动</Trans> : <Trans>节点已停止</Trans>
      }
      description={description}
      action={<StartNodeButton size="sm" testId="node-start-button-empty" />}
    />
  );
}
