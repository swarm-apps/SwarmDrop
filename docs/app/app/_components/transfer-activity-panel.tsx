"use client";

// #80 传输活动视图：projection 是生命周期主状态，progress 只补充实时速率/ETA/单文件进度。

import { RotateCcw } from "lucide-react";
import { useMemo, useState } from "react";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import { calcPercent, formatDuration, formatFileSize, formatTransferRate } from "../_lib/format";
import { getNode } from "../_lib/node-runtime";
import { useWebNode } from "../_lib/store";
import { toWebError, type Device, type TransferProgressEvent, type TransferProjection, type WebError } from "../_lib/view-types";

type TransferFileRow = TransferProgressEvent["files"][number] | TransferProjection["files"][number];

const PHASE_META: Record<
  TransferProjection["phase"],
  { label: string; dot: string; active: boolean }
> = {
  offered: { label: "等待处理", dot: "bg-amber-500", active: true },
  waiting_accept: { label: "等待对方接受", dot: "bg-amber-500", active: true },
  active: { label: "传输中", dot: "bg-emerald-500", active: true },
  suspended: { label: "已中断", dot: "bg-sky-500", active: true },
  terminal: { label: "已结束", dot: "bg-fd-muted-foreground", active: false },
};

const DIRECTION_LABEL: Record<TransferProjection["direction"], string> = {
  send: "发送",
  receive: "接收",
};

const CONNECTION_LABEL: Record<NonNullable<Device["connection"]>, string> = {
  lan: "局域网",
  dcutr: "打洞直连",
  relay: "中继",
};

const SUSPENDED_LABEL: Record<NonNullable<TransferProjection["suspendedReason"]>, string> = {
  local_paused: "本机暂停",
  remote_paused: "对方暂停",
  interrupted: "连接中断",
  peer_offline: "对方离线",
  app_restarted: "应用重启",
};

const TERMINAL_LABEL: Record<NonNullable<TransferProjection["terminalReason"]>, string> = {
  completed: "已完成",
  cancelled: "已取消",
  rejected: "已拒绝",
  fatal_error: "失败",
};

function sortByUpdatedDesc(items: TransferProjection[]) {
  return [...items].sort((a, b) => b.updatedAt - a.updatedAt);
}

function groupSessions(projections: Record<string, TransferProjection>) {
  const active: TransferProjection[] = [];
  const history: TransferProjection[] = [];

  for (const projection of sortByUpdatedDesc(Object.values(projections))) {
    if (PHASE_META[projection.phase].active) active.push(projection);
    else if (history.length < 8) history.push(projection);
  }

  return { active, history, total: active.length + history.length };
}

function connectionByPeer(devices: Device[]) {
  return new Map(devices.map((device) => [device.peerId, device.connection]));
}

function transferPercent(projection: TransferProjection, progress?: TransferProgressEvent): number {
  return calcPercent(progress?.transferredBytes ?? projection.transferredBytes, progress?.totalBytes ?? projection.totalSize);
}

function connectionLabel(projection: TransferProjection, connections: Map<string, Device["connection"]>): string {
  const connection = connections.get(projection.peerId);
  return connection ? CONNECTION_LABEL[connection] : "连接类型未知";
}

function phaseLabel(projection: TransferProjection): string {
  if (projection.phase === "suspended" && projection.suspendedReason) {
    return SUSPENDED_LABEL[projection.suspendedReason];
  }
  if (projection.phase === "terminal" && projection.terminalReason) {
    return TERMINAL_LABEL[projection.terminalReason];
  }
  return PHASE_META[projection.phase].label;
}

function elapsedSeconds(projection: TransferProjection): number | null {
  const end = projection.finishedAt ?? projection.updatedAt;
  if (!projection.startedAt || !end || end < projection.startedAt) return null;
  return Math.round((end - projection.startedAt) / 1000);
}

export function TransferActivityPanel() {
  const projections = useWebNode((s) => s.projections);
  const progress = useWebNode((s) => s.progress);
  const devices = useWebNode((s) => s.pairedDevices);
  const nodeStatus = useWebNode((s) => s.status);

  const [resumingIds, setResumingIds] = useState<Set<string>>(new Set());
  const [resumeErrors, setResumeErrors] = useState<Record<string, WebError>>({});

  const { active, history, total } = useMemo(() => groupSessions(projections), [projections]);
  const connections = useMemo(() => connectionByPeer(devices), [devices]);
  const ready = nodeStatus === "running";

  const resume = async (sessionId: string) => {
    const node = getNode();
    if (!node) return;

    setResumingIds((prev) => new Set(prev).add(sessionId));
    setResumeErrors((prev) => {
      const next = { ...prev };
      delete next[sessionId];
      return next;
    });
    try {
      await node.resume(sessionId);
    } catch (e) {
      setResumeErrors((prev) => ({ ...prev, [sessionId]: toWebError(e) }));
    } finally {
      setResumingIds((prev) => {
        const next = new Set(prev);
        next.delete(sessionId);
        return next;
      });
    }
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-fd-foreground">传输活动</h2>
        <p className="text-xs text-fd-muted-foreground">
          {active.length} 个进行中 · {history.length} 个已结束
        </p>
      </div>

      {total === 0 ? (
        <p className="mt-2 text-xs text-fd-muted-foreground">还没有传输会话。</p>
      ) : (
        <div className="mt-3 space-y-4">
          {active.length > 0 && (
            <ul className="space-y-2">
              {active.map((item) => (
                <TransferActivityItem
                  key={item.sessionId}
                  projection={item}
                  progress={progress[item.sessionId]}
                  connection={connectionLabel(item, connections)}
                  resumePending={resumingIds.has(item.sessionId)}
                  resumeError={resumeErrors[item.sessionId]}
                  resumeDisabled={!ready}
                  onResume={() => resume(item.sessionId)}
                />
              ))}
            </ul>
          )}

          {history.length > 0 && (
            <div className="border-t border-fd-border pt-4">
              <p className="text-xs font-medium text-fd-muted-foreground">最近完成</p>
              <ul className="mt-2 space-y-2">
                {history.map((item) => (
                  <TransferActivityItem
                    key={item.sessionId}
                    projection={item}
                    progress={progress[item.sessionId]}
                    connection={connectionLabel(item, connections)}
                    resumePending={resumingIds.has(item.sessionId)}
                    resumeError={resumeErrors[item.sessionId]}
                    resumeDisabled={!ready}
                    onResume={() => resume(item.sessionId)}
                  />
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function TransferActivityItem({
  projection,
  progress,
  connection,
  resumePending,
  resumeError,
  resumeDisabled,
  onResume,
}: {
  projection: TransferProjection;
  progress?: TransferProgressEvent;
  connection: string;
  resumePending: boolean;
  resumeError?: WebError;
  resumeDisabled: boolean;
  onResume: () => void;
}) {
  const percent = transferPercent(projection, progress);
  const phase = phaseLabel(projection);
  const bytesDone = progress?.transferredBytes ?? projection.transferredBytes;
  const totalBytes = progress?.totalBytes ?? projection.totalSize;
  const files: TransferFileRow[] = progress?.files ?? projection.files;
  const visibleFiles = files.slice(0, 4);
  const hiddenFileCount = files.length - visibleFiles.length;

  return (
    <li className="rounded-lg border border-fd-border bg-fd-background px-3 py-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <StatusDot colorClass={PHASE_META[projection.phase].dot} pulse={projection.phase === "active"} />
            <p className="truncate text-xs font-medium text-fd-foreground">
              {DIRECTION_LABEL[projection.direction]} · {projection.peerName}
            </p>
            <span className="rounded-full border border-fd-border px-2 py-0.5 text-[11px] text-fd-muted-foreground">
              {connection}
            </span>
          </div>
          <p className="mt-1 font-mono text-[11px] text-fd-muted-foreground">{projection.sessionId}</p>
        </div>
        <div className="text-right text-xs">
          <p className="font-medium text-fd-foreground">{phase}</p>
          <p className="mt-1 text-fd-muted-foreground">
            {formatTransferRate(progress?.speed)} · ETA {formatDuration(progress?.eta)}
          </p>
        </div>
      </div>

      <div className="mt-3">
        <div className="flex items-center justify-between gap-3 text-xs text-fd-muted-foreground">
          <span>
            {formatFileSize(bytesDone)} / {formatFileSize(totalBytes)}
          </span>
          <span>{percent}%</span>
        </div>
        <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-fd-border">
          <div className="h-full bg-[var(--brand-solid)] transition-[width]" style={{ width: `${percent}%` }} />
        </div>
      </div>

      <div className="mt-3 grid gap-1.5">
        {visibleFiles.map((file) => {
          const done = "transferred" in file ? file.transferred : file.transferredBytes;
          const filePercent = calcPercent(done, file.size);
          return (
            <div key={file.fileId} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 text-[11px]">
              <span className="truncate text-fd-foreground">{file.name}</span>
              <span className="font-mono text-fd-muted-foreground">
                {formatFileSize(done)} / {formatFileSize(file.size)} · {filePercent}%
              </span>
            </div>
          );
        })}
        {hiddenFileCount > 0 && <p className="text-[11px] text-fd-muted-foreground">还有 {hiddenFileCount} 个文件</p>}
      </div>

      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-fd-muted-foreground">
        <span>已用 {formatDuration(elapsedSeconds(projection))}</span>
        {projection.recoverable && (
          <button
            type="button"
            onClick={onResume}
            disabled={resumeDisabled || resumePending}
            className="inline-flex items-center gap-1.5 rounded-lg border border-fd-border px-2.5 py-1 font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
          >
            <RotateCcw className="size-3" aria-hidden="true" />
            {resumePending ? "续传中" : "续传"}
          </button>
        )}
      </div>
      {projection.errorMessage && <p className="mt-2 text-xs text-red-600 dark:text-red-400">{projection.errorMessage}</p>}
      {resumeError && <WebErrorCard error={resumeError} className="mt-2 text-xs" />}
    </li>
  );
}
