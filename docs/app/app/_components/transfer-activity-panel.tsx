"use client";

// #80 传输活动视图：projection 是生命周期主状态，progress 只补充实时速率/ETA/单文件进度。
//
// #93 选中态放 URL search param（`/app/transfer/?session=…`），刷新与分享后停在同一条会话上。
// **不能改用 `/app/transfer/[id]` 动态路由段**：docs 是 `output: "export"` 静态导出，动态段要
// `generateStaticParams` 预生成，而 sessionId 是运行时 UUID，永远预生成不出来。
// 详情就地展开而非另开页面：一次传输的信息量撑不起一整屏，展开足够，也省掉一次跳转。

import { Inbox, RotateCcw, Trash2, XCircle } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { type ReactNode, Suspense, memo, useCallback, useEffect, useMemo, useState } from "react";
import { ConfirmAction, INLINE_ACTION_CLASS, useConfirmAction } from "./confirm-action";
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
  isDeletableSession,
  sessionEndedAt,
  sortByUpdatedDesc,
} from "../_lib/format";
import { NAV, PARAM, inboxItemHref, transferSessionHref } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
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
  const cancelAction = useKeyedAsyncAction();
  const deleteAction = useKeyedAsyncAction();
  const clearAction = useAsyncAction();

  // 排序只依赖 projections；裁剪才依赖选中项。分两个 memo，点选不会连排序一起重跑。
  const sorted = useMemo(() => sortByUpdatedDesc(Object.values(projections)), [projections]);
  const { active, history, total } = useMemo(() => groupSessions(sorted, selectedId), [sorted, selectedId]);
  const connections = useMemo(() => connectionByPeer(devices), [devices]);
  const ready = nodeStatus === "running";

  // 二次确认走 banner 形态：触发按钮在头部与计数并排，确认横幅在头部下方撑满整张卡，
  // 两段节点分处两地，所以用 hook 而不是 <ConfirmAction />。
  const clearConfirm = useConfirmAction({
    icon: Trash2,
    label: "清空记录",
    pendingLabel: "清空中",
    confirmLabel: "确认清空",
    // 清空不可撤销，但它删的只是账本，文案必须把这条说清楚，
    // 否则用户会以为收到的文件也一起没了。
    warning: "只清空已结束的记录；已接收的文件仍在收件箱，不受影响。",
    layout: "banner",
    disabled: !ready,
    pending: clearAction.pending,
    onConfirm: () => {
      const node = getNode();
      if (!node) return;
      clearAction.run(
        () => node.clear_transfer_history(),
        () => webNodeActions.clearTerminalProjections(),
      );
    },
  });

  // 三个动作都先取一次 node；取不到就静默返回——按钮已由 `ready` 禁用，这里只是兜底。
  const resume = useCallback(
    (sessionId: string) => {
      const node = getNode();
      if (!node) return;
      void resumeAction.run(sessionId, () => node.resume(sessionId));
    },
    [resumeAction.run],
  );

  // 方向由 projection 直接给出，不靠「先试 cancel_send，失败再试 cancel_receive」去猜：
  // 取消是有副作用的操作（发 wire 帧、删半成品、写终态），拿它当探针会在第一条真失败时
  // 顺手对另一个方向也来一遍。
  //
  // 取消成功后**不动任何本地状态**：终态由内核经 TransferProjection 事件回流
  //（_lib/event-dispatch.ts），前端抢着改只会和回流的那份打架。
  const cancel = useCallback(
    (sessionId: string, direction: TransferProjection["direction"]) => {
      const node = getNode();
      if (!node) return;
      void cancelAction.run(sessionId, () =>
        direction === "send" ? node.cancel_send(sessionId) : node.cancel_receive(sessionId),
      );
    },
    [cancelAction.run],
  );

  // 删除是**账本操作**：只清这一条记录，OPFS 里已接收的文件仍在收件箱里能看能下载。
  //
  // 与取消/续传的关键差别是**没有回流事件**——会话被删掉之后内核不会再为它发 projection，
  // 所以成功后必须由前端把它从投影域摘掉，否则要等下一次刷新才消失（#104）。
  const remove = useCallback(
    (sessionId: string) => {
      const node = getNode();
      if (!node) return;
      void deleteAction.run(sessionId, async () => {
        await node.delete_transfer_session(sessionId);
        webNodeActions.removeProjection(sessionId);
      });
    },
    [deleteAction.run],
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

  const renderItem = (item: TransferProjection) => {
    const sessionId = item.sessionId;
    const expanded = sessionId === selectedId;
    return (
      <TransferActivityItem
        key={sessionId}
        projection={item}
        progress={progress[sessionId]}
        connection={connectionLabel(item, connections)}
        expanded={expanded}
        onSelect={select}
        // 未展开就不建动作区：`null` 是稳定引用，那些项的 memo 照旧不被打穿。展开的那一项
        // 每次都拿到新 element 因而必然重渲染——它同时也是在逐帧显示进度明细的那一项，
        // 本来就每帧都要重画，这里没有新增负担。
        actions={
          expanded ? (
            <TransferItemActions
              projection={item}
              ready={ready}
              resume={{
                pending: resumeAction.isPending(sessionId),
                error: resumeAction.errorFor(sessionId),
                run: () => resume(sessionId),
              }}
              cancel={{
                pending: cancelAction.isPending(sessionId),
                error: cancelAction.errorFor(sessionId),
                run: () => cancel(sessionId, item.direction),
              }}
              remove={{
                pending: deleteAction.isPending(sessionId),
                error: deleteAction.errorFor(sessionId),
                run: () => remove(sessionId),
              }}
            />
          ) : null
        }
      />
    );
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-fd-foreground">会话</h2>
        <div className="flex items-center gap-3">
          <p className="text-xs text-fd-muted-foreground">
            {active.length} 个进行中 · {history.length} 个已结束
          </p>
          {history.length > 0 && clearConfirm.trigger}
        </div>
      </div>

      {clearConfirm.panel}
      {clearAction.error && <WebErrorCard error={clearAction.error} className="mt-2 text-xs" />}

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
  actions,
}: {
  projection: TransferProjection;
  progress?: TransferProgressEvent;
  connection: string;
  expanded: boolean;
  onSelect: (sessionId: string, isExpanded: boolean) => void;
  /**
   * 展开时的动作区（续传 / 取消 / 删除 + 各自的错误）。由父组件构造并在未展开时传 `null`——
   * 三个动作的 pending/error/回调本来要 12 个 prop 从这里纯转发下去，而它们只在展开时可见，
   * 且同时只有一项展开。放进 slot 之后这个组件对动作零依赖，memo 的比较面也小了一圈。
   */
  actions: ReactNode;
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

          {actions}
        </div>
      )}
    </li>
  );
});

/** 单个动作对外的全部状态：pending / error 来自调用方的 async-action hook，`run` 已绑好会话。 */
type ItemAction = {
  pending: boolean;
  error?: WebError;
  run: () => void;
};

// 展开项的动作区。抽成组件而不是留在 TransferActivityItem 里，是为了让那个 memo 组件对
// 「有几个动作、各自什么状态」彻底无感——加一个动作不再是给它加四个 prop。
//
// 它只在展开时挂载，所以确认态会随折叠一起消失（此前 confirming 是 item 级 state，折叠
// 期间会赖着，再展开时用户看到的是上一次没点完的确认条）。
function TransferItemActions({
  projection,
  ready,
  resume,
  cancel,
  remove,
}: {
  projection: TransferProjection;
  /** 节点未就绪时三个动作都点不动——共用一个前置条件，不必各传一份。 */
  ready: boolean;
  resume: ItemAction;
  cancel: ItemAction;
  remove: ItemAction;
}) {
  return (
    <>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-fd-muted-foreground">
        <span>已用 {formatDuration(elapsedSeconds(projection))}</span>
        <div className="flex flex-wrap items-center gap-2">
          {/* 传输页是「过程」、收件箱是「结果」（分工见 inbox/page.tsx），此前两者是两座孤岛：
              一次接收在两处各出现一次，却没有任何一条边把它们连起来。 */}
          {projection.direction === "receive" && (
            <InboxItemLink sessionId={projection.sessionId} phase={projection.phase} ready={ready} />
          )}
          {/* 续传是可重试的幂等动作，不设二次确认——只有不可逆的那两个才拦。 */}
          {projection.recoverable && (
            <button
              type="button"
              onClick={resume.run}
              disabled={!ready || resume.pending}
              className={INLINE_ACTION_CLASS}
            >
              <RotateCcw className="size-3" aria-hidden="true" />
              {resume.pending ? "续传中" : "续传"}
            </button>
          )}
          {/* 判据用 isActiveSession——导航徽标与分组也用它，另写一份会在新增 phase 时对不上。 */}
          {isActiveSession(projection) && (
            <ConfirmAction
              icon={XCircle}
              label="取消"
              pendingLabel="取消中"
              confirmLabel="确认取消"
              // 取消是不可逆的终态动作，却与「续传」并排——误点的代价不对称。
              warning="取消后无法恢复"
              disabled={!ready}
              pending={cancel.pending}
              onConfirm={cancel.run}
            />
          )}
          {/* 只在可删的两种 phase 露出（判据与内核守卫同源）。进行中的会话要先取消——
              它有活 actor 还在写 checkpoint，删掉记录只会留下孤儿。 */}
          {isDeletableSession(projection) && (
            <ConfirmAction
              icon={Trash2}
              label="删除"
              pendingLabel="删除中"
              confirmLabel="确认删除"
              // suspended 那条连断点一起没，代价比删一条普通记录大，得分开说。
              warning={
                projection.phase === "suspended"
                  ? "断点信息将一并清除，无法再续传；已接收的文件仍在收件箱"
                  : "只删这条记录，已接收的文件仍在收件箱"
              }
              disabled={!ready}
              pending={remove.pending}
              onConfirm={remove.run}
            />
          )}
        </div>
      </div>
      {projection.errorMessage && <p className="mt-2 text-xs text-red-600 dark:text-red-400">{projection.errorMessage}</p>}
      {resume.error && <WebErrorCard error={resume.error} className="mt-2 text-xs" />}
      {cancel.error && <WebErrorCard error={cancel.error} className="mt-2 text-xs" />}
      {remove.error && <WebErrorCard error={remove.error} className="mt-2 text-xs" />}
    </>
  );
}

/**
 * 「查看收到的文件」——按会话反查收件箱条目（`inbox_item_by_session`）。
 *
 * 查询挂在这里而不是列表层，因为动作区**只在展开时挂载**：一次只查一条，用户不展开就不查。
 * 提到列表层就变成「每渲染一次列表，对每条接收会话各查一次」。
 *
 * 查不到就整个不渲染（返回 `null`），不留占位也不报错——这是**常态而非异常**：发送方向没有
 * 条目、接收未完成没有条目、条目被用户删掉了也没有。为一件正常缺席的事挂一行「暂无」，
 * 会让每条已取消的接收记录下面都多一句废话。
 *
 * 依赖里带 `phase` 是因为「接收中」展开时反查必然为空，而条目是在完成那一刻才建的——
 * 不重查的话，用户得收起再展开才看得到链接。
 */
function InboxItemLink({
  sessionId,
  phase,
  ready,
}: {
  sessionId: string;
  phase: TransferProjection["phase"];
  /** 节点未就绪时 `getNode()` 为 null；带上它，就绪后这条 effect 会重跑而不是永远空着。 */
  ready: boolean;
}) {
  /** 反查结果：`undefined` = 还没查完，`null` = 确实没有对应条目。 */
  const [target, setTarget] = useState<{ id: string; archived: boolean } | null | undefined>(undefined);

  useEffect(() => {
    const node = getNode();
    if (!node) return;
    let cancelled = false;
    void (async () => {
      try {
        const detail = await node.inbox_item_by_session(sessionId);
        if (cancelled) return;
        setTarget(detail ? { id: detail.id, archived: detail.archivedAt !== null } : null);
      } catch (e) {
        // 反查失败只是少一条快捷链接，收件箱页本身照常可达，不值得占用会话的错误位。
        console.error("[web] inbox_item_by_session() 失败", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionId, phase, ready]);

  if (!target) return null;
  return (
    // 归档状态一并编进链接：只带 id 的链接到不了已归档的条目（收件箱默认不显示它们）。
    // 反查回来的本就是完整 detail，这里不用它就得让落地页去猜。
    <Link href={inboxItemHref(target.id, target.archived)} className={INLINE_ACTION_CLASS}>
      <Inbox className="size-3" aria-hidden="true" />
      查看收到的文件
    </Link>
  );
}
