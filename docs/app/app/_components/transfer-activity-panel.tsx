"use client";

// #80 传输活动视图：projection 是生命周期主状态，progress 只补充实时速率/ETA/单文件进度。
//
// #93 选中态放 URL search param（`/app/transfer/?session=…`），刷新与分享后停在同一条会话上。
// **不能改用 `/app/transfer/[id]` 动态路由段**：docs 是 `output: "export"` 静态导出，动态段要
// `generateStaticParams` 预生成，而 sessionId 是运行时 UUID，永远预生成不出来。
// 详情就地展开而非另开页面：一次传输的信息量撑不起一整屏，展开足够，也省掉一次跳转。

import { RotateCcw } from "lucide-react";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, memo, useCallback, useMemo } from "react";
import { PanelFallback } from "./panel-fallback";
import { ProgressBar } from "./progress-bar";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import {
  calcPercent,
  formatDuration,
  formatFileSize,
  formatTransferRate,
  isActiveSession,
  sessionEndedAt,
  sortByUpdatedDesc,
} from "../_lib/format";
import { NAV, PARAM, transferSessionHref } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { useWebNode } from "../_lib/store";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { type Device, type TransferProgressEvent, type TransferProjection, type WebError } from "../_lib/view-types";

type TransferFileRow = TransferProgressEvent["files"][number] | TransferProjection["files"][number];

/** 已结束会话的展示条数——再多就该去收件箱看结果，而不是在活动列表里翻页。 */
const HISTORY_LIMIT = 8;

// 只管展示（标签 + 状态点色）。「是否进行中」的判定在 `_lib/format.ts` 的 `isActiveSession`——
// 导航徽标也用它，两处各写一份就会在新增 phase 时对不上。
const PHASE_META: Record<TransferProjection["phase"], { label: string; dot: string }> = {
  offered: { label: "等待处理", dot: "bg-amber-500" },
  waiting_accept: { label: "等待对方接受", dot: "bg-amber-500" },
  active: { label: "传输中", dot: "bg-emerald-500" },
  suspended: { label: "已中断", dot: "bg-sky-500" },
  terminal: { label: "已结束", dot: "bg-fd-muted-foreground" },
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

/**
 * 分组并裁剪历史。入参已排好序——排序只依赖 projections，别和只影响裁剪的 `selectedId`
 * 挤进同一个 memo，否则每点一下选中都要重跑一次全量排序。
 *
 * `selectedId` 参与裁剪是必需的：发送页「查看传输」带过来的 session 可能早已排在第 20 条，
 * 若被 HISTORY_LIMIT 截掉，用户点链接进来会看到一个「什么都没选中」的列表。
 */
function groupSessions(sorted: TransferProjection[], selectedId: string | null) {
  const active: TransferProjection[] = [];
  const history: TransferProjection[] = [];

  for (const projection of sorted) {
    if (isActiveSession(projection)) active.push(projection);
    else if (history.length < HISTORY_LIMIT || projection.sessionId === selectedId) history.push(projection);
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
  const end = sessionEndedAt(projection);
  if (!projection.startedAt || !end || end < projection.startedAt) return null;
  return Math.round((end - projection.startedAt) / 1000);
}

export function TransferActivityPanel() {
  return (
    <Suspense fallback={<PanelFallback>正在读取传输会话…</PanelFallback>}>
      <TransferActivityPanelInner />
    </Suspense>
  );
}

function TransferActivityPanelInner() {
  const projections = useWebNode((s) => s.projections);
  const progress = useWebNode((s) => s.progress);
  const devices = useWebNode((s) => s.pairedDevices);
  const nodeStatus = useWebNode((s) => s.status);

  const router = useRouter();
  const selectedId = useSearchParams().get(PARAM.session);

  const resumeAction = useKeyedAsyncAction();

  // 排序只依赖 projections；裁剪才依赖选中项。分两个 memo，点选不会连排序一起重跑。
  const sorted = useMemo(() => sortByUpdatedDesc(Object.values(projections)), [projections]);
  const { active, history, total } = useMemo(() => groupSessions(sorted, selectedId), [sorted, selectedId]);
  const connections = useMemo(() => connectionByPeer(devices), [devices]);
  const ready = nodeStatus === "running";

  // 引用稳定：列表项是 memo 的，每次渲染新建回调会让 memo 彻底失效。
  const resume = useCallback(
    (sessionId: string) => {
      const node = getNode();
      if (!node) return;
      void resumeAction.run(sessionId, () => node.resume(sessionId));
    },
    [resumeAction.run],
  );

  // 选中态只改 URL，不另存一份 state——刷新、前进后退、分享链接都自动一致。
  // `replace` 而非 `push`：在列表里点几下不该在浏览器历史里堆出几十条记录。
  // `scroll: false`：就地展开时页面不该跳回顶部。
  //
  // 「当前是否已选中」由子组件回传，而不是在这里闭包捕获 `selectedId`：捕获的话每点一下
  // select 就换一次引用，传给 memo 化的列表项后把整张表的 memo 打穿（实际只有两条的展开态变了）。
  const select = useCallback(
    (sessionId: string, isExpanded: boolean) => {
      router.replace(isExpanded ? NAV.transfer.href : transferSessionHref(sessionId), { scroll: false });
    },
    [router],
  );

  const renderItem = (item: TransferProjection) => (
    <TransferActivityItem
      key={item.sessionId}
      projection={item}
      progress={progress[item.sessionId]}
      connection={connectionLabel(item, connections)}
      expanded={item.sessionId === selectedId}
      onSelect={select}
      resumePending={resumeAction.isPending(item.sessionId)}
      resumeError={resumeAction.errorFor(item.sessionId)}
      resumeDisabled={!ready}
      onResume={resume}
    />
  );

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-fd-foreground">会话</h2>
        <p className="text-xs text-fd-muted-foreground">
          {active.length} 个进行中 · {history.length} 个已结束
        </p>
      </div>

      {total === 0 ? (
        <p className="mt-2 text-xs text-fd-muted-foreground">还没有传输会话。</p>
      ) : (
        <div className="mt-3 space-y-4">
          {active.length > 0 && <ul className="space-y-2">{active.map(renderItem)}</ul>}

          {history.length > 0 && (
            <div className="border-t border-fd-border pt-4">
              <p className="text-xs font-medium text-fd-muted-foreground">最近完成</p>
              <ul className="mt-2 space-y-2">{history.map(renderItem)}</ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// memo：store 逐 key immutable 更新 projections/progress，未变动的会话保持原引用——
// 一个会话每秒十余次的进度事件因此只重渲染它自己那一项，而不是整张活动 + 历史列表。
const TransferActivityItem = memo(function TransferActivityItem({
  projection,
  progress: liveProgress,
  connection,
  expanded,
  onSelect,
  resumePending,
  resumeError,
  resumeDisabled,
  onResume,
}: {
  projection: TransferProjection;
  progress?: TransferProgressEvent;
  connection: string;
  expanded: boolean;
  onSelect: (sessionId: string, isExpanded: boolean) => void;
  resumePending: boolean;
  resumeError?: WebError;
  resumeDisabled: boolean;
  onResume: (sessionId: string) => void;
}) {
  // `progress` 是**在途采样**：会话一进终态内核就不再下发，最后收到的那一帧会永远
  // 停在那儿。而下面每一处都让采样优先于 projection，于是终态被一个陈旧值盖住。
  //
  // projection 才是权威——它在终态已回填全量字节。2026-07-28 实测：16 MiB 传完、
  // `projection.transferredBytes` 已是 16777216，采样却停在 3932160，界面显示
  // 「已完成 · 23%」。
  const ended = projection.phase === "terminal";
  const progress = ended ? undefined : liveProgress;

  const percent = transferPercent(projection, progress);
  const phase = phaseLabel(projection);
  const bytesDone = progress?.transferredBytes ?? projection.transferredBytes;
  const totalBytes = progress?.totalBytes ?? projection.totalSize;
  const files: TransferFileRow[] = progress?.files ?? projection.files;

  return (
    <li
      className={`rounded-lg border bg-fd-background transition-colors ${
        expanded ? "border-[var(--brand)]/40" : "border-fd-border"
      }`}
    >
      {/* 整行是一个按钮（键盘可达 + aria-expanded），续传按钮放在它外面——button 不能嵌套。 */}
      <button
        type="button"
        onClick={() => onSelect(projection.sessionId, expanded)}
        aria-expanded={expanded}
        className="w-full cursor-pointer px-3 py-3 text-left"
      >
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
            {expanded && <p className="mt-1 font-mono text-[11px] break-all text-fd-muted-foreground">{projection.sessionId}</p>}
          </div>
          <div className="text-right text-xs">
            <p className="font-medium text-fd-foreground">{phase}</p>
            {/* 速率与 ETA 只对进行中的会话有意义——终态显示「等待数据 · ETA 未知」
                是在报告一个不存在的等待。已用时长在下方单独展示。 */}
            {!ended && (
              <p className="mt-1 text-fd-muted-foreground">
                {formatTransferRate(progress?.speed)} · ETA {formatDuration(progress?.eta)}
              </p>
            )}
          </div>
        </div>

        <div className="mt-3">
          <div className="flex items-center justify-between gap-3 text-xs text-fd-muted-foreground">
            <span>
              {formatFileSize(bytesDone)} / {formatFileSize(totalBytes)}
            </span>
            <span>{percent}%</span>
          </div>
          <ProgressBar percent={percent} className="mt-1.5" />
        </div>
      </button>

      {expanded && (
        <div className="border-t border-fd-border px-3 py-3">
          <div className="grid gap-1.5">
            {files.map((file) => {
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
          </div>

          <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-fd-muted-foreground">
            <span>已用 {formatDuration(elapsedSeconds(projection))}</span>
            {projection.recoverable && (
              <button
                type="button"
                onClick={() => onResume(projection.sessionId)}
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
        </div>
      )}
    </li>
  );
});
