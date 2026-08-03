"use client";

// #76 连接面板：填 helper 地址 → connect（拨出去，拿连接类型）/ 建立可达（登记 relay 意图，
// 浏览器唯一的被动接收入口）。两个动作共用同一个地址输入，默认预填一个公共 helper。
//
// 「可达」是**常驻意图**不是一次性动作（#84 后内核改成声明式：ensure 登记 → 后台持续收敛，
// until_active 只是等首次建立）。所以有 ensure 就必须有 drop——否则填错地址后后台会一直退避
// 重试，用户没有任何办法停下来。
//
// 连接类型（local/direct/relayed）是真实状态，要像安全工具一样清楚可视化（PRODUCT.md 原则
// 2），配色沿用桌面 device-card 的语义编码：局域网 emerald / 打洞 sky / 中继 amber
// （DESIGN.md：这三色是状态语义编码，在 one-accent 规则之外）。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import Link from "next/link";
import { useRef, useState } from "react";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import { getNode } from "../_lib/node-runtime";
import { NAV } from "../_lib/nav";
import { WEB_RELAY_HELPERS } from "../_lib/relay-helpers";
import { useAsyncAction } from "../_lib/use-async-action";
import { useWebNode, webNodeActions } from "../_lib/store";
import { type PathKindJson } from "../_lib/view-types";

const PATH_META: Record<PathKindJson, { label: MessageDescriptor; dot: string }> = {
  local: { label: msg`局域网直连`, dot: "bg-emerald-500" },
  direct: { label: msg`打洞直连`, dot: "bg-sky-500" },
  relayed: { label: msg`中继`, dot: "bg-amber-500" },
};

export function ConnectionPanel() {
  const { t } = useLingui();
  const nodeStatus = useWebNode((s) => s.status);
  const connection = useWebNode((s) => s.connection);
  const reservation = useWebNode((s) => s.reservation);

  const [addr, setAddr] = useState(() => WEB_RELAY_HELPERS[0] ?? "");
  const connectAction = useAsyncAction();
  const reserveAction = useAsyncAction();
  const dropAction = useAsyncAction();
  /** `relays_ensure` 返回的 helper NodeId——撤销意图要用它，不必再解析一遍 multiaddr。 */
  const helperIdRef = useRef<string | null>(null);

  const ready = nodeStatus === "running";

  const doConnect = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    connectAction.run(
      () => node.connect(addr.trim()),
      (conn) => webNodeActions.setConnection(conn),
    );
  };

  const doReserve = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    reserveAction.run(
      () => {
        const helperId = node.relays_ensure(addr.trim());
        helperIdRef.current = helperId;
        return node.relays_until_active(helperId);
      },
      (circuit) => webNodeActions.setReservation(circuit),
    );
  };

  const doDrop = () => {
    const node = getNode();
    const helperId = helperIdRef.current;
    if (!node || !helperId) return;
    dropAction.run(
      () => node.relays_drop(helperId),
      () => {
        helperIdRef.current = null;
        webNodeActions.setReservation(null);
      },
    );
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-foreground">
        <Trans>连接</Trans>
      </h2>
      <p className="mt-1 text-xs text-muted-foreground">
        <Trans>
          填一个 helper 的 <code className="font-mono">webrtc-direct</code> 地址（带{" "}
          <code className="font-mono">/p2p/&lt;id&gt;</code>{" "}
          尾段）。浏览器不 listen 本地 socket，要被对端拨回得先建立 circuit 可达地址。
        </Trans>
      </p>

      <input
        className="mt-3 w-full rounded-lg border border-fd-border bg-fd-background px-3 py-2 font-mono text-xs text-fd-foreground placeholder:text-fd-muted-foreground"
        placeholder="/ip4/.../udp/.../webrtc-direct/certhash/.../p2p/12D3Koo..."
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
          {connectAction.pending ? <Trans>连接中…</Trans> : "connect"}
        </button>
        <button
          type="button"
          onClick={doReserve}
          disabled={!ready || !addr.trim() || reserveAction.pending}
          className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
        >
          {reserveAction.pending ? <Trans>建立中…</Trans> : <Trans>建立可达（circuit）</Trans>}
        </button>
        {reservation && (
          <button
            type="button"
            onClick={doDrop}
            disabled={!ready || dropAction.pending}
            className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
          >
            {dropAction.pending ? <Trans>撤销中…</Trans> : <Trans>撤销</Trans>}
          </button>
        )}
      </div>

      {connectAction.error && <WebErrorCard error={connectAction.error} className="mt-3 text-xs" />}

      {connection && (
        <div className="mt-3 flex items-center gap-2 rounded-lg border border-fd-border bg-fd-background px-3 py-2">
          <StatusDot colorClass={PATH_META[connection.path].dot} />
          <span className="text-xs font-medium text-foreground">{t(PATH_META[connection.path].label)}</span>
          <span className="truncate font-mono text-xs text-fd-muted-foreground">{connection.addr}</span>
        </div>
      )}

      {reserveAction.error && <WebErrorCard error={reserveAction.error} className="mt-3 text-xs" />}
      {dropAction.error && <WebErrorCard error={dropAction.error} className="mt-3 text-xs" />}

      {reservation && (
        <div className="mt-3">
          <p className="text-xs font-medium text-muted-foreground">
            <Trans>
              circuit 可达地址（供{" "}
              <Link href={NAV.devices.href} className="underline underline-offset-2">
                设备
              </Link>{" "}
              页的「生成邀请」使用，刷新前有效）
            </Trans>
          </p>
          <p className="mt-1 break-all rounded-lg border border-fd-border bg-fd-background px-3 py-2 font-mono text-xs text-fd-foreground">
            {reservation}
          </p>
        </div>
      )}
    </div>
  );
}
