"use client";

// dev 事件面板（次要、默认折叠）：证明「事件流接上、事件进 store、零丢弃」，非主 UI 反馈。
// 结构化渲染在各自的面板（连接/发送/传输活动/接收），这里只留原始事件名与各域计数——
// 排查「界面没反应」时第一眼看它：事件流为 0 而传输有条目，就说明数据来自启动回补而非实时流。
//
// **只在开发构建里渲染。** 它自己就写着「非主 UI 反馈」，却一直是生产设置页的第四张卡，
// 与主题、语言、节点身份这些真·用户设置并排——把内部诊断摆在用户设置里，既让设置页显得
// 更碎，也在暗示用户这是他该关心的东西。`NODE_ENV` 在打包时是常量，整块会被 tree-shake 掉。

import { Trans } from "@lingui/react/macro";
import { useWebNode } from "../_lib/store";

export function DevEventLog() {
  // 守卫必须在**任何 hook 之前**。四条订阅原本排在它上面，于是生产构建里它们照样挂着：
  // 这四个域都是高频的（事件流每条事件都变），store 一动就重渲染一次、再 `return null`。
  // 关进内部组件后守卫才真的省掉订阅，也给 bundler 摇掉整个面板的机会。
  if (process.env.NODE_ENV === "production") return null;
  return <DevEventLogPanel />;
}

function DevEventLogPanel() {
  const eventLog = useWebNode((s) => s.eventLog);
  const offers = useWebNode((s) => s.offers);
  const projections = useWebNode((s) => s.projections);
  const pendingPairings = useWebNode((s) => s.pendingPairings);

  return (
    <details className="rounded-xl border bg-card/50 p-4">
      <summary className="cursor-pointer text-xs font-medium text-muted-foreground">
        <Trans>
          事件流 {eventLog.length} · offer {Object.keys(offers).length} · 传输{" "}
          {Object.keys(projections).length} · 配对请求 {pendingPairings.length}
        </Trans>
      </summary>
      <ul className="mt-3 space-y-1 font-mono text-xs text-muted-foreground">
        {eventLog.length === 0 && <li><Trans>（暂无事件）</Trans></li>}
        {eventLog
          .slice(-12)
          .reverse()
          .map((ev, i) => (
            <li key={`${eventLog.length}-${i}`}>{ev.type}</li>
          ))}
      </ul>
    </details>
  );
}
