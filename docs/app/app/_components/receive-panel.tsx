"use client";

// #79 接收面板：入站 offer 以非阻断通知形式浮现在页面里（不做「发送/接收」Tab 切换，
// PRODUCT.md 原则 1）——accept_offer/reject_offer 决策；已完成接收的会话在下方「收件箱」
// 展示（原则 2·状态诚实可见：落盘完成才给下载），点下载才读回 OPFS 建 blob URL。
// 收件箱跨刷新仍在（#81）：内核把已完成的接收会话持久化到 IndexedDB，启动时经
// `transfer_history()` 回补进 projections，文件本体一直在 OPFS。
//
// 未配对对端的 offer 由内核在协议层硬拒 NotPaired，从不进入 `pending_offers()`——本机对此
// 零可见性（符合「安全边界」设计，不是本面板的缺陷）。#79 验收标准里的「需先配对」提示因此
// 落在发送侧：见 send-panel.tsx 消费 `rejections` 域。

import { WebErrorCard } from "./web-error-view";
import { formatFileSize, sessionEndedAt } from "../_lib/format";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { type TransferProjection } from "../_lib/view-types";

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

type ProjectionFile = TransferProjection["files"][number];

export function ReceivePanel() {
  const offers = useWebNode((s) => s.offers);
  const projections = useWebNode((s) => s.projections);

  const decideAction = useKeyedAsyncAction();
  const downloadAction = useKeyedAsyncAction();

  const offerList = Object.values(offers);
  // 显式按收到时间倒序：projections 混着实时事件与 #81 的启动回补，靠对象 key 的插入顺序
  // 会让刷新前后的收件箱排法不一致。
  const completed = Object.values(projections)
    .filter((p) => p.direction === "receive" && p.phase === "terminal" && p.terminalReason === "completed")
    .sort((a, b) => sessionEndedAt(b) - sessionEndedAt(a));

  const download = (sessionId: string, file: ProjectionFile) => {
    const node = getNode();
    if (!node) return;
    void downloadAction.run(`${sessionId}:${file.fileId}`, async () => {
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
        <h2 className="text-sm font-semibold text-fd-foreground">接收</h2>
        {offerList.length > 0 && (
          <p className="rounded-full bg-fd-accent px-2 py-0.5 text-xs font-medium text-fd-foreground" role="status" aria-live="polite">
            {offerList.length} 个待处理请求
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

      <div className="mt-5 border-t border-fd-border pt-4">
        <p className="text-xs font-medium text-fd-muted-foreground">收件箱</p>
        {completed.length === 0 ? (
          <p className="mt-2 text-xs text-fd-muted-foreground">还没有收到的文件。</p>
        ) : (
          <ul className="mt-2 space-y-2">
            {completed.map((p) => (
              <li key={p.sessionId} className="rounded-lg border border-fd-border bg-fd-background px-3 py-2">
                <p className="truncate text-xs text-fd-foreground">
                  来自 <span className="font-medium">{p.peerName}</span>
                </p>
                <ul className="mt-1 space-y-1">
                  {p.files.map((f) => {
                    const key = `${p.sessionId}:${f.fileId}`;
                    const error = downloadAction.errorFor(key);
                    return (
                      <li key={f.fileId} className="text-xs">
                        <div className="flex items-center justify-between gap-2">
                          <span className="truncate text-fd-foreground">{f.name}</span>
                          <span className="flex shrink-0 items-center gap-2">
                            <span className="font-mono text-fd-muted-foreground">{formatFileSize(f.size)}</span>
                            <button
                              type="button"
                              onClick={() => download(p.sessionId, f)}
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
    </div>
  );
}
