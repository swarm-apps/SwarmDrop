"use client";

// #78 发送面板：选已配对设备 → 拖拽/选择文件 → send_files() → prepare 进度实时展示。
// 隐式优先 + 极客高效（PRODUCT.md 原则 1/3）：选设备、拖文件、送达，路径尽量短，不堆确认
// 层级；prepare 是真实阶段而非假 loading（原则 2·状态诚实可见）。
// 发送完成（Offer 已发出）后的实时进度视图交给 ⑥（#80）；本面板只多接了 #79 的一项——对方拒绝
// （尤其未配对硬拒 NotPaired）不能悬空成永远的「已发出」，见下方 rejection 分支。

import { useRef, useState } from "react";
import { WebErrorCard } from "./web-error-view";
import { calcPercent, formatFileSize } from "../_lib/format";
import { getNode } from "../_lib/node-runtime";
import { useAsyncAction } from "../_lib/use-async-action";
import { useWebNode } from "../_lib/store";
import { OFFER_REJECT_REASON_LABEL } from "../_lib/view-types";

/** 待发送文件项——配 id 而非数组下标做 key，移除文件时不会因下标前移而错位复用。 */
interface PendingFile {
  id: number;
  file: File;
}

let nextFileId = 0;

export function SendPanel() {
  const nodeStatus = useWebNode((s) => s.status);
  const devices = useWebNode((s) => s.pairedDevices);
  const prepareProgress = useWebNode((s) => s.latestPrepareProgress);
  const ready = nodeStatus === "running";

  const [peerId, setPeerId] = useState("");
  const [files, setFiles] = useState<PendingFile[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const [sentSessionId, setSentSessionId] = useState<string | null>(null);
  const sendAction = useAsyncAction();
  // #79：对方拒绝了刚发出的 offer（尤其未配对硬拒 NotPaired）——不能让「已发出」成功态
  // 永久悬空显示，必须替换成清晰的拒绝提示。
  const rejection = useWebNode((s) => (sentSessionId ? s.rejections[sentSessionId] : undefined));
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const addFiles = (list: FileList | null) => {
    if (!list || list.length === 0) return;
    setFiles((prev) => [...prev, ...Array.from(list, (file) => ({ id: nextFileId++, file }))]);
    setSentSessionId(null);
  };

  const removeFile = (id: number) => {
    setFiles((prev) => prev.filter((f) => f.id !== id));
  };

  const doSend = () => {
    const node = getNode();
    if (!node || !peerId || files.length === 0) return;
    sendAction.run(
      () => node.send_files(peerId, files.map((f) => f.file)),
      (sessionId) => {
        setSentSessionId(sessionId);
        setFiles([]);
      },
    );
  };

  const canSend = ready && !!peerId && files.length > 0 && !sendAction.pending;

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-fd-foreground">发送</h2>

      {devices.length === 0 ? (
        <p className="mt-2 text-xs text-fd-muted-foreground">
          还没有已配对设备，先在上方「配对」区完成配对。
        </p>
      ) : (
        <>
          <select
            className="mt-3 w-full rounded-lg border border-fd-border bg-fd-background px-3 py-2 text-xs text-fd-foreground"
            value={peerId}
            onChange={(e) => setPeerId(e.target.value)}
            // 选设备/移除文件不受「是否已选目标」约束，故不复用 canSend——只挡节点未就绪/发送中。
            disabled={!ready || sendAction.pending}
          >
            <option value="">选择设备…</option>
            {devices.map((d) => (
              <option key={d.peerId} value={d.peerId} disabled={d.status !== "online"}>
                {d.name ?? d.hostname}
                {d.status !== "online" ? "（离线）" : ""}
              </option>
            ))}
          </select>

          <div
            className={`mt-3 rounded-lg border-2 border-dashed px-4 py-6 text-center text-xs transition-colors ${
              dragOver ? "border-[var(--brand-solid)] bg-fd-accent" : "border-fd-border"
            }`}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(true);
            }}
            onDragLeave={() => setDragOver(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragOver(false);
              addFiles(e.dataTransfer.files);
            }}
          >
            <p className="text-fd-muted-foreground">
              拖拽文件到此处，或{" "}
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                className="font-medium text-fd-foreground underline underline-offset-2"
              >
                选择文件
              </button>
            </p>
            <input
              ref={fileInputRef}
              type="file"
              multiple
              className="hidden"
              onChange={(e) => {
                addFiles(e.target.files);
                e.target.value = "";
              }}
            />
          </div>

          {files.length > 0 && (
            <ul className="mt-2 space-y-1">
              {files.map(({ id, file }) => (
                <li
                  key={id}
                  className="flex items-center justify-between gap-2 rounded-lg border border-fd-border bg-fd-background px-3 py-1.5 text-xs"
                >
                  <span className="truncate text-fd-foreground">{file.name}</span>
                  <span className="flex shrink-0 items-center gap-2">
                    <span className="font-mono text-fd-muted-foreground">{formatFileSize(file.size)}</span>
                    <button
                      type="button"
                      onClick={() => removeFile(id)}
                      disabled={sendAction.pending}
                      className="text-fd-muted-foreground hover:text-fd-foreground disabled:opacity-50"
                      aria-label={`移除 ${file.name}`}
                    >
                      ×
                    </button>
                  </span>
                </li>
              ))}
            </ul>
          )}

          <button
            type="button"
            onClick={doSend}
            disabled={!canSend}
            className="mt-3 rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
          >
            {sendAction.pending ? "发送中…" : "发送"}
          </button>

          {sendAction.pending && prepareProgress && (
            <div className="mt-3">
              <p className="text-xs text-fd-muted-foreground">
                正在准备 {prepareProgress.currentFile}（{prepareProgress.completedFiles}/
                {prepareProgress.totalFiles} 文件 · {formatFileSize(prepareProgress.bytesHashed)}/
                {formatFileSize(prepareProgress.totalBytes)}）
              </p>
              <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-fd-border">
                <div
                  className="h-full bg-[var(--brand-solid)]"
                  style={{
                    width: `${calcPercent(prepareProgress.bytesHashed, prepareProgress.totalBytes)}%`,
                  }}
                />
              </div>
            </div>
          )}

          {sendAction.error && <WebErrorCard error={sendAction.error} className="mt-3 text-xs" />}
          {sentSessionId && !rejection && (
            <p className="mt-3 text-xs text-emerald-600 dark:text-emerald-400">
              已发出：<span className="font-mono">{sentSessionId}</span>（对方接受后即可传输）
            </p>
          )}
          {rejection && (
            <p className="mt-3 text-xs text-amber-600 dark:text-amber-400">
              {rejection.reason ? OFFER_REJECT_REASON_LABEL[rejection.reason.type] : "对方拒绝了此次传输"}
            </p>
          )}
        </>
      )}
    </div>
  );
}
