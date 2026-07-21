"use client";

// dev 事件面板（次要、默认折叠）：证明「事件流接上、三类事件进 store、零丢弃」，非主 UI 反馈。
// 结构化渲染（进度/连接/传输视图）归后续模块（#76~#80）。

import { useWebNode } from "../_lib/store";

export function DevEventLog() {
  const eventLog = useWebNode((s) => s.eventLog);
  const offers = useWebNode((s) => s.offers);
  const projections = useWebNode((s) => s.projections);
  const pendingPairings = useWebNode((s) => s.pendingPairings);

  return (
    <details className="rounded-xl border border-fd-border bg-fd-card/50 p-4">
      <summary className="cursor-pointer text-xs font-medium text-fd-muted-foreground">
        事件流 {eventLog.length} · offer {Object.keys(offers).length} · 传输{" "}
        {Object.keys(projections).length} · 配对请求 {pendingPairings.length}
      </summary>
      <ul className="mt-3 space-y-1 font-mono text-xs text-fd-muted-foreground">
        {eventLog.length === 0 && <li>（暂无事件）</li>}
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
