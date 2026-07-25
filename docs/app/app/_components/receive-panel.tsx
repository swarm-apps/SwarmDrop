"use client";

// #79 接收面板：入站 offer 以非阻断通知形式浮现在页面里（不做「发送/接收」Tab 切换，
// PRODUCT.md 原则 1）——accept_offer/reject_offer 决策；已完成接收的会话在下方「收件箱」
// 展示文件与下载链接（读回 OPFS 建 blob URL，原则 2·状态诚实可见：落盘完成才给下载）。
//
// 未配对对端的 offer 由内核在协议层硬拒 NotPaired，从不进入 `pending_offers()`——本机对此
// 零可见性（符合「安全边界」设计，不是本面板的缺陷）。#79 验收标准里的「需先配对」提示因此
// 落在发送侧：见 send-panel.tsx 消费 `rejections` 域。

import { useEffect, useRef, useState } from "react";
import { WebErrorCard } from "./web-error-view";
import { formatFileSize } from "../_lib/format";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { toWebError, type TransferProjection, type WebError, type WebNode } from "../_lib/view-types";

interface DownloadEntry {
  fileId: number;
  name: string;
  url: string;
}

interface DownloadResolution {
  entries: DownloadEntry[];
  error: WebError | null;
}

/** 收到的文件读回 OPFS 建 blob URL——逐文件独立 try，一个失败不挡其余文件下载。 */
async function resolveDownloads(node: WebNode, projection: TransferProjection): Promise<DownloadResolution> {
  const entries: DownloadEntry[] = [];
  let error: WebError | null = null;
  for (const f of projection.files) {
    try {
      entries.push({ fileId: f.fileId, name: f.name, url: await node.download_url(f.relativePath) });
    } catch (e) {
      console.error(`[web] download_url(${f.relativePath}) 失败`, e);
      error ??= toWebError(e);
    }
  }
  return { entries, error };
}

export function ReceivePanel() {
  const offers = useWebNode((s) => s.offers);
  const projections = useWebNode((s) => s.projections);

  const [decidingIds, setDecidingIds] = useState<Set<string>>(new Set());
  const [decideError, setDecideError] = useState<WebError | null>(null);
  const [downloads, setDownloads] = useState<Record<string, DownloadEntry[]>>({});
  const [downloadErrors, setDownloadErrors] = useState<Record<string, WebError>>({});
  const resolvedRef = useRef<Set<string>>(new Set());

  const offerList = Object.values(offers);
  const completed = Object.values(projections).filter(
    (p) => p.direction === "receive" && p.phase === "terminal" && p.terminalReason === "completed",
  );

  // 每当有新完成的接收会话，惰性拉一次 download_url——resolvedRef 保证同一 session 只拉一次
  // （projections 更新会重跑 effect，未完成/已解析过的会话在循环里被跳过）。
  useEffect(() => {
    const node = getNode();
    if (!node) return;
    const done = Object.values(projections).filter(
      (p) => p.direction === "receive" && p.phase === "terminal" && p.terminalReason === "completed",
    );
    for (const p of done) {
      if (resolvedRef.current.has(p.sessionId)) continue;
      resolvedRef.current.add(p.sessionId);
      void resolveDownloads(node, p).then(({ entries, error }) => {
        setDownloads((prev) => ({ ...prev, [p.sessionId]: entries }));
        if (error) {
          setDownloadErrors((prev) => ({ ...prev, [p.sessionId]: error }));
        }
      });
    }
  }, [projections]);

  const retryDownloads = (sessionId: string) => {
    const node = getNode();
    const projection = projections[sessionId];
    if (!node || !projection) return;
    resolvedRef.current.delete(sessionId);
    setDownloadErrors((prev) => {
      const next = { ...prev };
      delete next[sessionId];
      return next;
    });
    resolvedRef.current.add(sessionId);
    void resolveDownloads(node, projection).then(({ entries, error }) => {
      setDownloads((prev) => ({ ...prev, [sessionId]: entries }));
      if (error) setDownloadErrors((prev) => ({ ...prev, [sessionId]: error }));
    });
  };

  const decide = async (sessionId: string, accept: boolean) => {
    const node = getNode();
    if (!node) return;
    setDecidingIds((prev) => new Set(prev).add(sessionId));
    setDecideError(null);
    try {
      if (accept) await node.accept_offer(sessionId);
      else await node.reject_offer(sessionId);
      webNodeActions.removeOffer(sessionId);
    } catch (e) {
      setDecideError(toWebError(e));
    } finally {
      setDecidingIds((prev) => {
        const next = new Set(prev);
        next.delete(sessionId);
        return next;
      });
    }
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
                  disabled={decidingIds.has(offer.sessionId)}
                  className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
                >
                  接受
                </button>
                <button
                  type="button"
                  onClick={() => decide(offer.sessionId, false)}
                  disabled={decidingIds.has(offer.sessionId)}
                  className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
                >
                  拒绝
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      {decideError && <WebErrorCard error={decideError} className="mt-2 text-xs" />}

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
                    const url = downloads[p.sessionId]?.find((d) => d.fileId === f.fileId)?.url;
                    return (
                      <li key={f.fileId} className="flex items-center justify-between gap-2 text-xs">
                        <span className="truncate text-fd-foreground">{f.name}</span>
                        <span className="flex shrink-0 items-center gap-2">
                          <span className="font-mono text-fd-muted-foreground">{formatFileSize(f.size)}</span>
                          {url ? (
                            <a
                              href={url}
                              download={f.name}
                              className="font-medium text-fd-foreground underline underline-offset-2"
                            >
                              下载
                            </a>
                          ) : (
                            <span className="text-fd-muted-foreground">准备中…</span>
                          )}
                        </span>
                      </li>
                    );
                  })}
                </ul>
                {downloadErrors[p.sessionId] && (
                  <div className="mt-2">
                    <WebErrorCard error={downloadErrors[p.sessionId]} className="text-xs" />
                    <button
                      type="button"
                      onClick={() => retryDownloads(p.sessionId)}
                      className="mt-2 text-xs font-medium text-fd-foreground underline underline-offset-2"
                    >
                      重试生成下载链接
                    </button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
