"use client";

// 已配对设备清单（#77 验收标准之一）。presence 在线状态诚实可见（PRODUCT.md 原则 2）——
// 快照来自 state-poll.ts 的定时刷新，非事件驱动（`paired_devices()` 是同步查询非事件流）。
//
// 每行的「发送」是设备页在信息架构里的角色所在（#91）：设备关系是起点，发送是它的出口。
// 只对在线设备出链接——离线设备在发送页也是 disabled 的，给个点不动的入口只会浪费一次点击。

import Link from "next/link";
import { sendToPeerHref } from "../_lib/nav";
import { useWebNode } from "../_lib/store";
import { StatusDot } from "./status-dot";

export function DeviceList() {
  const devices = useWebNode((s) => s.pairedDevices);

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-fd-foreground">已配对设备</h2>
      {devices.length === 0 ? (
        <p className="mt-2 text-xs text-fd-muted-foreground">
          暂无已配对设备。在下方「配对」区消费一条邀请即可。
        </p>
      ) : (
        <ul className="mt-3 space-y-2">
          {devices.map((d) => (
            <li
              key={d.peerId}
              className="flex items-center justify-between gap-2 rounded-lg border border-fd-border bg-fd-background px-3 py-2"
            >
              <div className="min-w-0">
                <p className="truncate text-xs font-medium text-fd-foreground">{d.name ?? d.hostname}</p>
                <p className="truncate font-mono text-[11px] text-fd-muted-foreground">{d.peerId}</p>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <span className="inline-flex items-center gap-1.5 rounded-full border border-fd-border px-2 py-0.5 text-[11px] font-medium text-fd-muted-foreground">
                  <StatusDot colorClass={d.status === "online" ? "bg-emerald-500" : "bg-fd-muted-foreground"} />
                  {d.status === "online" ? "在线" : "离线"}
                </span>
                {d.status === "online" && (
                  <Link
                    href={sendToPeerHref(d.peerId)}
                    className="rounded-lg border border-fd-border px-2.5 py-1 text-[11px] font-medium text-fd-foreground transition-colors hover:bg-fd-accent"
                  >
                    发送
                  </Link>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
