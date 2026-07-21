"use client";

// #76 连接面板：填 helper 地址 → connect（拨出去，拿连接类型）/ reserve（建 circuit
// reservation，浏览器唯一的被动接收入口）。两个动作共用同一个地址输入，与 /try 页一致。
//
// 连接类型（local/direct/relayed）是真实状态，要像安全工具一样清楚可视化（PRODUCT.md 原则
// 2），配色沿用桌面 device-card 的语义编码：局域网 emerald / 打洞 sky / 中继 amber
// （DESIGN.md：这三色是状态语义编码，在 one-accent 规则之外）。

import { useState } from "react";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import { getNode } from "../_lib/node-runtime";
import { useAsyncAction } from "../_lib/use-async-action";
import { useWebNode, webNodeActions } from "../_lib/store";
import { type PathKindJson } from "../_lib/view-types";
import { withTimeout } from "../_lib/with-timeout";

const PATH_META: Record<PathKindJson, { label: string; dot: string }> = {
  local: { label: "局域网直连", dot: "bg-emerald-500" },
  direct: { label: "打洞直连", dot: "bg-sky-500" },
  relayed: { label: "中继", dot: "bg-amber-500" },
};

/**
 * 实测对不可达地址 `reserve()` 会随 swarm 拨号重试无限期挂起（`connect()` 至少还会在数十秒
 * 后 reject）——wasm 侧的 Promise 没有内建超时。不加这层，UI 会卡在「reserve 中…」永不恢复，
 * 违反「连接失败有清晰反馈，不静默」。这是纯前端兜底，不改内核重试语义（内核可能后续仍在
 * 背后重试，只是 UI 不再等它）。
 */
const ACTION_TIMEOUT_MS = 20_000;

export function ConnectionPanel() {
  const nodeStatus = useWebNode((s) => s.status);
  const connection = useWebNode((s) => s.connection);
  const reservation = useWebNode((s) => s.reservation);

  const [addr, setAddr] = useState("");
  const connectAction = useAsyncAction();
  const reserveAction = useAsyncAction();

  const ready = nodeStatus === "running";

  const doConnect = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    connectAction.run(
      () =>
        withTimeout(node.connect(addr.trim()), ACTION_TIMEOUT_MS, {
          kind: "network",
          message: `connect 超时（${ACTION_TIMEOUT_MS / 1000}s 内未响应，helper 可能不可达）`,
        }),
      (conn) => webNodeActions.setConnection(conn),
    );
  };

  const doReserve = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    reserveAction.run(
      () =>
        withTimeout(node.reserve(addr.trim()), ACTION_TIMEOUT_MS, {
          kind: "network",
          message: `reserve 超时（${ACTION_TIMEOUT_MS / 1000}s 内未建立可达性，helper 可能不可达或仍在后台重试拨号）`,
        }),
      (circuit) => webNodeActions.setReservation(circuit),
    );
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-fd-foreground">连接</h2>
      <p className="mt-1 text-xs text-fd-muted-foreground">
        填一个 helper 的 <code className="font-mono">ws</code> 或{" "}
        <code className="font-mono">webrtc-direct</code> 地址（带 <code className="font-mono">/p2p/&lt;id&gt;</code>{" "}
        尾段）。浏览器不 listen 本地 socket，被对端拨回需先 reserve 拿到 circuit 地址。
      </p>

      <input
        className="mt-3 w-full rounded-lg border border-fd-border bg-fd-background px-3 py-2 font-mono text-xs text-fd-foreground placeholder:text-fd-muted-foreground"
        placeholder="/ip4/.../tcp/.../ws/p2p/12D3Koo... 或 .../webrtc-direct/certhash/.../p2p/..."
        value={addr}
        onChange={(e) => setAddr(e.target.value)}
        disabled={!ready}
      />

      <div className="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          onClick={doConnect}
          disabled={!ready || !addr.trim() || connectAction.pending}
          className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
        >
          {connectAction.pending ? "连接中…" : "connect"}
        </button>
        <button
          type="button"
          onClick={doReserve}
          disabled={!ready || !addr.trim() || reserveAction.pending}
          className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
        >
          {reserveAction.pending ? "reserve 中…" : "reserve（circuit listen）"}
        </button>
      </div>

      {connectAction.error && <WebErrorCard error={connectAction.error} className="mt-3 text-xs" />}

      {connection && (
        <div className="mt-3 flex items-center gap-2 rounded-lg border border-fd-border bg-fd-background px-3 py-2">
          <StatusDot colorClass={PATH_META[connection.path].dot} />
          <span className="text-xs font-medium text-fd-foreground">{PATH_META[connection.path].label}</span>
          <span className="truncate font-mono text-xs text-fd-muted-foreground">{connection.addr}</span>
        </div>
      )}

      {reserveAction.error && <WebErrorCard error={reserveAction.error} className="mt-3 text-xs" />}

      {reservation && (
        <div className="mt-3">
          <p className="text-xs font-medium text-fd-muted-foreground">
            circuit 可达地址（供 ③配对生成邀请使用，刷新前有效）
          </p>
          <p className="mt-1 break-all rounded-lg border border-fd-border bg-fd-background px-3 py-2 font-mono text-xs text-fd-foreground">
            {reservation}
          </p>
        </div>
      )}
    </div>
  );
}
