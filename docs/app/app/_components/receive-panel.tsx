"use client";

// #79 接收：入站 offer 以非阻断通知形式浮现（不做「发送/接收」Tab 切换，PRODUCT.md 原则 1）——
// accept_offer/reject_offer 决策；已接收内容在「收件箱」展示（原则 2·状态诚实可见：
// 落盘完成才给下载），点下载才读回 OPFS 建 blob URL。
// 收件箱跨刷新仍在：内核侧是一张**独立的 IndexedDB 表**（不再是「过滤已完成接收会话投影」），
// 前端经 `inbox_items()` 读它，文件本体一直在 OPFS。
// 换真表带来两处行为差异，都是修正而非回归：清空传输历史不再清掉收件箱，传输历史触到
// 100 条上限被淘汰时收件箱条目也不会跟着消失。
//
// 未配对对端的 offer 由内核在协议层硬拒 NotPaired，从不进入 `pending_offers()`——本机对此
// 零可见性（符合「安全边界」设计，不是本面板的缺陷）。#79 验收标准里的「需先配对」提示因此
// 落在发送侧：见 send-panel.tsx 消费 `rejections` 域。
//
// #96 拆成两块并挂到 /app/inbox：待处理请求是**决策**（有时效），收件箱是**结果**（可回看）。
// 两者混在一个卡里时，一条永久列表会把一条限时动作压下去。多路由后请求的可见性由导航徽标
// 兜底（见 app-nav.tsx），用户不在收件箱页也知道有东西等着。

import { useEffect, useState } from "react";
import { WebErrorCard } from "./web-error-view";
import { formatFileSize } from "../_lib/format";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { toWebError, type InboxItemDetail, type InboxItemFileEntry, type WebError } from "../_lib/view-types";

/**
 * blob URL 的存活窗口。
 *
 * 一个 blob URL 会钉住它背后那份 OPFS 文件快照直到被 revoke 或页面关闭——收件箱累计几个 GB，
 * 就是几个 GB 的浏览器存储回收不掉。所以下载链接**点了才生成、用完就撤**，而不是进页面就把
 * 整个收件箱解析一遍（#81 把历史灌进 projections 后，那会变成上百次 OPFS 句柄操作 + 上百个
 * 永不回收的 URL）。撤销给足 30 秒：`a.click()` 触发后浏览器读取 blob 是异步的，立刻 revoke
 * 会让大文件下载中途失败。
 */
const BLOB_URL_TTL_MS = 30_000;

export function IncomingOffersPanel() {
  const offers = useWebNode((s) => s.offers);
  const decideAction = useKeyedAsyncAction();

  const offerList = Object.values(offers);

  const decide = (sessionId: string, accept: boolean) => {
    const node = getNode();
    if (!node) return;
    void decideAction.run(sessionId, async () => {
      if (accept) await node.accept_offer(sessionId);
      else await node.reject_offer(sessionId);
      webNodeActions.removeOffer(sessionId);
    });
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-fd-foreground">待处理请求</h2>
        {offerList.length > 0 && (
          <p
            className="rounded-full bg-fd-accent px-2 py-0.5 text-xs font-medium text-fd-foreground"
            role="status"
            aria-live="polite"
          >
            {offerList.length} 个待处理
          </p>
        )}
      </div>

      {offerList.length === 0 ? (
        <p className="mt-2 text-xs text-fd-muted-foreground">暂无待处理的入站文件请求。</p>
      ) : (
        <ul className="mt-3 space-y-2">
          {offerList.map((offer) => (
            <li key={offer.sessionId} className="rounded-lg border border-fd-border bg-fd-background px-3 py-2">
              <p className="text-xs text-fd-foreground">
                <span className="font-medium">{offer.deviceName}</span> 想发送 {offer.files.length} 个文件（
                {formatFileSize(offer.totalSize)}）
              </p>
              <ul className="mt-1 space-y-0.5">
                {offer.files.map((f) => (
                  <li key={f.fileId} className="truncate font-mono text-[11px] text-fd-muted-foreground">
                    {f.name}（{formatFileSize(f.size)}）
                  </li>
                ))}
              </ul>
              <div className="mt-2 flex gap-2">
                <button
                  type="button"
                  onClick={() => decide(offer.sessionId, true)}
                  disabled={decideAction.isPending(offer.sessionId)}
                  className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
                >
                  接受
                </button>
                <button
                  type="button"
                  onClick={() => decide(offer.sessionId, false)}
                  disabled={decideAction.isPending(offer.sessionId)}
                  className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
                >
                  拒绝
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      {decideAction.latestError && <WebErrorCard error={decideAction.latestError} className="mt-2 text-xs" />}
    </div>
  );
}

/**
 * 收件箱真表的拉取。
 *
 * `inbox_items()` 直接给出 `InboxItemDetail[]`（文件行与传输投影都在同一把锁下批量补好），
 * 一次调用就够——本面板要逐文件下载，正好吃这份完整结构。
 *
 * 拉取入口刻意留在本组件内，**不下放到 `WebNodeBootstrap`**——那里是运行时单例的位置
 * （spawn 节点、接事件流），不是数据拉取的位置；收件箱只有这一页要看。
 */
async function fetchInboxItems(): Promise<InboxItemDetail[]> {
  const node = getNode();
  if (!node) return [];
  return node.inbox_items(false);
}

export function InboxPanel() {
  const items = useWebNode((s) => s.inboxItems);
  const status = useWebNode((s) => s.status);
  const inboxRevision = useWebNode((s) => s.inboxRevision);
  const [loadError, setLoadError] = useState<WebError | null>(null);
  const downloadAction = useKeyedAsyncAction();

  // 挂载时（节点就绪后）拉一次 + 每次「接收方向的传输完成」重拉一次。收件箱是低频写入，
  // 一次 `inbox_items()` 的成本远低于为它新造一条订阅式推送通道。
  useEffect(() => {
    if (status !== "running") return;
    let cancelled = false;
    void (async () => {
      try {
        const details = await fetchInboxItems();
        if (cancelled) return;
        webNodeActions.setInboxItems(details);
        setLoadError(null);
      } catch (e) {
        if (cancelled) return;
        console.error("[web] inbox_items() 失败", e);
        setLoadError(toWebError(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [status, inboxRevision]);

  const download = (itemId: string, file: InboxItemFileEntry) => {
    const node = getNode();
    if (!node) return;
    void downloadAction.run(`${itemId}:${file.id}`, async () => {
      try {
        const url = await node.download_url(file.relativePath);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = file.name;
        anchor.click();
        setTimeout(() => URL.revokeObjectURL(url), BLOB_URL_TTL_MS);
      } catch (e) {
        console.error(`[web] download_url(${file.relativePath}) 失败`, e);
        throw e;
      }
    });
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-fd-foreground">已接收</h2>
      {loadError && <WebErrorCard error={loadError} className="mt-2 text-xs" />}
      {items.length === 0 ? (
        <p className="mt-2 text-xs text-fd-muted-foreground">还没有收到的文件。</p>
      ) : (
        <ul className="mt-3 space-y-2">
          {items.map((item) => (
            <li key={item.id} className="rounded-lg border border-fd-border bg-fd-background px-3 py-2">
              <p className="truncate text-xs text-fd-foreground">
                来自 <span className="font-medium">{item.sourceName}</span>
              </p>
              <ul className="mt-1 space-y-1">
                {item.files.map((f) => {
                  const key = `${item.id}:${f.id}`;
                  const error = downloadAction.errorFor(key);
                  return (
                    <li key={f.id} className="text-xs">
                      <div className="flex items-center justify-between gap-2">
                        <span className="truncate text-fd-foreground">{f.name}</span>
                        <span className="flex shrink-0 items-center gap-2">
                          <span className="font-mono text-fd-muted-foreground">{formatFileSize(f.size)}</span>
                          <button
                            type="button"
                            onClick={() => download(item.id, f)}
                            disabled={downloadAction.isPending(key)}
                            className="font-medium text-fd-foreground underline underline-offset-2 disabled:opacity-50"
                          >
                            {downloadAction.isPending(key) ? "准备中…" : error ? "重试下载" : "下载"}
                          </button>
                        </span>
                      </div>
                      {error && <WebErrorCard error={error} className="mt-1 text-xs" />}
                    </li>
                  );
                })}
              </ul>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
