"use client";

// 已配对设备清单（#77 验收标准之一）。presence 在线状态诚实可见（PRODUCT.md 原则 2）——
// 快照来自 state-poll.ts 的定时刷新，非事件驱动（`paired_devices()` 是同步查询非事件流）。
//
// 每行的「发送」是设备页在信息架构里的角色所在（#91）：设备关系是起点，发送是它的出口。
// 只对在线设备出链接——离线设备在发送页也是 disabled 的，给个点不动的入口只会浪费一次点击。
//
// 「取消配对」用**行内二次确认**（#100）：点击后该行就地换成「确认取消配对 / 取消」，
// 不引入模态——应用区没有 dialog 原语，为一句确认引一套 focus trap 不划算。解除本身走内核的
// `remove_paired_device()`（core 的原子 unpair：先落盘 → 再删共享内存表 → 再发事件），
// 所以失败时设备**还在**清单里、错误也说得出来，不会出现「点完就没了、刷新又回来」。
//
// ## 为什么这份确认不用 `ConfirmAction`（#109 的结论：不合并）
//
// 应用区有两个确认原语，这是刻意的，不是没来得及收口。两者在**同一个轴上取了相反的值**：
//
// | | `ConfirmAction`（清空历史 / 取消传输 / 删除记录） | 这一份 |
// |---|---|---|
// | 复位时机 | 点确认的**同一拍**，不等异步结果 | `await` 成功**之后** |
// | 失败后 | 已经复位了（压根没等结果） | **留在确认态**，错误就地显示，重试一次点击 |
// | confirming | `boolean`，组件自持 | `string \| null`，记的是哪一行 |
//
// `ConfirmAction` 的核心设计就是「不等异步」——它的注释写明了理由：等结果会让确认条在整个
// 请求期间赖着不走，多出一个此前没有的中间态。而这一份**要求**知道成败，因为取消配对是一次
// 可失败的网络操作（节点没起来、内核报错都可能），失败后让用户重新点两次（触发 → 确认）
// 才能重试是惩罚。
//
// 给 `ConfirmAction` 加个「失败保持确认态」开关就得把 `error` 也纳进去，而那三个调用点的
// 错误卡位置各不相同（有的在横幅下方独立一行、有的与 `projection.errorMessage` 有固定先后），
// 纳进组件就会挪动它们的展示位置。让调用方控制 confirming 则等于退回收口前的样子。
//
// 判据：**动作失败后用户还想不想留在「我要删这个」的状态里**。想 → 用这一份的形态；
// 不想（失败率低、重来成本也低）→ 用 `ConfirmAction`。

import Link from "next/link";
import { useState } from "react";
import { sendToPeerHref } from "../_lib/nav";
import { deviceDisplayName } from "../_lib/device-name";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";

export function DeviceList() {
  const devices = useWebNode((s) => s.pairedDevices);
  /** 正在二次确认的那一行；同一时刻至多一行——确认是即时决定，不该能攒出一批待办。 */
  const [confirmingPeerId, setConfirmingPeerId] = useState<string | null>(null);
  // 逐行独立的 pending 与错误，与收件箱下载、活动续传同一形态。
  const unpairAction = useKeyedAsyncAction();

  const doUnpair = (peerId: string) => {
    const node = getNode();
    if (!node) return;
    void unpairAction.run(peerId, async () => {
      await node.remove_paired_device(peerId);
      setConfirmingPeerId(null);
      // 内核那边这台设备已经从共享表里摘掉了，这里立刻取一份新快照——等下一轮
      // state-poll 的话，点完之后它还会在列表里挂最多 1.5 秒，看着像是没生效。
      webNodeActions.setPairedDevices(node.paired_devices());
    });
    // 失败时**不清** confirmingPeerId：那一行保持在确认态、错误就地显示，重试只差一次点击。
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-fd-foreground">已配对设备</h2>
      {devices.length === 0 ? (
        <p className="mt-2 text-xs text-fd-muted-foreground">
          暂无已配对设备。在下方「配对」区消费一条邀请即可。
        </p>
      ) : (
        <ul className="mt-3 space-y-2">
          {devices.map((d) => {
            const confirming = confirmingPeerId === d.peerId;
            const pending = unpairAction.isPending(d.peerId);
            const error = unpairAction.errorFor(d.peerId);
            return (
              <li
                key={d.peerId}
                className="rounded-lg border border-fd-border bg-fd-background px-3 py-2"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0">
                    <p className="truncate text-xs font-medium text-fd-foreground">{deviceDisplayName(d)}</p>
                    <p className="truncate font-mono text-[11px] text-fd-muted-foreground">{d.peerId}</p>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    {/* 在线状态一直在场：确认态下也别让它跳没了，行高会抖。 */}
                    <span className="inline-flex items-center gap-1.5 rounded-full border border-fd-border px-2 py-0.5 text-[11px] font-medium text-fd-muted-foreground">
                      <StatusDot colorClass={d.status === "online" ? "bg-emerald-500" : "bg-fd-muted-foreground"} />
                      {d.status === "online" ? "在线" : "离线"}
                    </span>
                    {confirming ? (
                      <>
                        <button
                          type="button"
                          onClick={() => doUnpair(d.peerId)}
                          disabled={pending}
                          className="rounded-lg border border-red-500/40 px-2.5 py-1 text-[11px] font-medium text-red-700 transition-colors hover:bg-red-50 disabled:opacity-50 dark:text-red-300 dark:hover:bg-red-950/40"
                        >
                          {pending ? "取消配对中…" : "确认取消配对"}
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirmingPeerId(null)}
                          disabled={pending}
                          className="rounded-lg border border-fd-border px-2.5 py-1 text-[11px] font-medium text-fd-muted-foreground transition-colors hover:bg-fd-accent disabled:opacity-50"
                        >
                          取消
                        </button>
                      </>
                    ) : (
                      <>
                        {/* 确认态下不出「发送」：destructive 确认旁边摆一个正向入口只会招误点。 */}
                        {d.status === "online" && (
                          <Link
                            href={sendToPeerHref(d.peerId)}
                            className="rounded-lg border border-fd-border px-2.5 py-1 text-[11px] font-medium text-fd-foreground transition-colors hover:bg-fd-accent"
                          >
                            发送
                          </Link>
                        )}
                        <button
                          type="button"
                          onClick={() => setConfirmingPeerId(d.peerId)}
                          className="rounded-lg border border-fd-border px-2.5 py-1 text-[11px] font-medium text-fd-muted-foreground transition-colors hover:bg-fd-accent"
                        >
                          取消配对
                        </button>
                      </>
                    )}
                  </div>
                </div>
                {confirming && (
                  <p className="mt-2 text-xs text-fd-muted-foreground" aria-live="polite">
                    取消后需要重新配对才能传输文件。
                  </p>
                )}
                {error && <WebErrorCard error={error} className="mt-2 text-xs" />}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
