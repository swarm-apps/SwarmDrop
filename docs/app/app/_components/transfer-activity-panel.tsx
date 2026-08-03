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
import {
  type ReactNode,
  Suspense,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ConfirmAction, INLINE_ACTION_CLASS, useConfirmAction } from "./confirm-action";
import { PanelFallback } from "./panel-fallback";
import { ProgressBar } from "./progress-bar";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import {
  isActiveSession,
  isDeletableSession,
  sessionEndedAt,
  sortByUpdatedDesc,
} from "../_lib/format";
import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import {
  calcPercent,
  formatDuration,
  formatFileSize,
  formatTransferRate,
} from "@swarmdrop/shared-view";
import { cn } from "@/lib/cn";
import { MasterDetail, OpenListButton } from "./master-detail";
import { PARAM, inboxItemHref, transferSessionHref } from "../_lib/nav";
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
const PHASE_META: Record<TransferProjection["phase"], { label: MessageDescriptor; dot: string }> = {
  offered: { label: msg`等待处理`, dot: "bg-amber-500" },
  waiting_accept: { label: msg`等待对方接受`, dot: "bg-amber-500" },
  active: { label: msg`传输中`, dot: "bg-emerald-500" },
  suspended: { label: msg`已中断`, dot: "bg-sky-500" },
  terminal: { label: msg`已结束`, dot: "bg-muted-foreground" },
};

const DIRECTION_LABEL: Record<TransferProjection["direction"], MessageDescriptor> = {
  send: msg`发送`,
  receive: msg`接收`,
};

const CONNECTION_LABEL: Record<NonNullable<Device["connection"]>, MessageDescriptor> = {
  lan: msg`局域网`,
  dcutr: msg`打洞直连`,
  relay: msg`中继`,
};

const SUSPENDED_LABEL: Record<NonNullable<TransferProjection["suspendedReason"]>, MessageDescriptor> = {
  local_paused: msg`本机暂停`,
  remote_paused: msg`对方暂停`,
  interrupted: msg`连接中断`,
  peer_offline: msg`对方离线`,
  app_restarted: msg`应用重启`,
};

const TERMINAL_LABEL: Record<NonNullable<TransferProjection["terminalReason"]>, MessageDescriptor> = {
  completed: msg`已完成`,
  cancelled: msg`已取消`,
  rejected: msg`已拒绝`,
  fatal_error: msg`失败`,
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

/**
 * 未知连接方式的兜底标签。**必须是模块级常量**：`msg` 宏每次求值都新建一个对象，写在
 * `connectionLabel` 的返回位上会让它每次调用返回新引用——而这个值是 `TransferActivityItem`
 * 的 prop，那个组件靠 `memo` 让「每秒十余次的进度事件只重渲染它自己那一行」。
 * 新引用会把整张表的 memo 打穿。
 */
const UNKNOWN_CONNECTION_LABEL = msg`连接类型未知`;

/**
 * 下面两个返回**描述符**而非字符串：它们是模块级纯函数，翻译宏在这里只能定义、不能展开
 * （展开要 `useLingui()`，那是组件的事）。调用点拿到描述符自己 `t(...)`。
 *
 * 两者的返回值都必须是**稳定引用**（见上），所以只从模块级的映射表里取，不现造。
 */
function connectionLabel(
  projection: TransferProjection,
  connections: Map<string, Device["connection"]>,
): MessageDescriptor {
  const connection = connections.get(projection.peerId);
  return connection ? CONNECTION_LABEL[connection] : UNKNOWN_CONNECTION_LABEL;
}

function phaseLabel(projection: TransferProjection): MessageDescriptor {
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
    <Suspense fallback={<PanelFallback>
        <Trans>正在读取传输会话…</Trans>
      </PanelFallback>}>
      <TransferActivityPanelInner />
    </Suspense>
  );
}

function TransferActivityPanelInner() {
  const { t } = useLingui();
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
    label: t`清空记录`,
    pendingLabel: t`清空中`,
    confirmLabel: t`确认清空`,
    // 清空不可撤销，但它删的只是账本，文案必须把这条说清楚，
    // 否则用户会以为收到的文件也一起没了。
    warning: t`只清空已结束的记录；已接收的文件仍在收件箱，不受影响。`,
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
  // `scroll: false`：切换详情时页面不该跳回顶部。
  //
  // **主从布局下点选即选中，不再 toggle**：此前是手风琴，点已展开的那条会收起；现在详情是
  // 独立的一栏（窄屏下更是整屏），把它「收起」只会留下一个空面板，没有任何意义。
  //
  // **必须是稳定引用**：它是 `TransferActivityItem` 的 prop，而那个组件靠 `memo` 让一个会话
  // 每秒十余次的进度事件只重渲染它自己那一行。每帧新建的箭头函数会把整张表的 memo 打穿。
  // 窄屏下选中后还要收起抽屉，而 `closeDrawer` 只在 `list` 渲染回调里拿得到——用 ref 中转，
  // 别把它并进依赖（那样每次渲染又换一次引用，等于没 memo）。
  const closeDrawerRef = useRef<() => void>(() => {});
  const select = useCallback(
    (sessionId: string) => {
      router.replace(transferSessionHref(sessionId), { scroll: false });
      closeDrawerRef.current();
    },
    [router],
  );

  const selected = selectedId ? projections[selectedId] : undefined;

  // `onSelect` 传的是稳定的 `select`（收抽屉由它内部经 ref 完成，见上）——**不要**在这里
  // 现造箭头函数，那会打穿 `TransferActivityItem` 的 memo。
  const renderRow = (item: TransferProjection) => (
    <TransferActivityItem
      key={item.sessionId}
      projection={item}
      progress={progress[item.sessionId]}
      connection={connectionLabel(item, connections)}
      selected={item.sessionId === selectedId}
      onSelect={select}
    />
  );

  return (
    <MasterDetail
      testId="transfer-master-detail"
      drawerLabel={t`传输会话列表`}
      list={({ closeDrawer }) => {
        // 渲染期写 ref：`select` 要在窄屏下顺带收抽屉，但 `closeDrawer` 只在这个回调里拿得到。
        // 与 `master-detail.tsx` 里 `onCloseRef` 是同一个手法，理由也一样——不能进依赖。
        closeDrawerRef.current = closeDrawer;
        return (
        <div className="flex min-h-0 flex-col gap-3 p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-sm font-semibold text-foreground">
              <Trans>会话</Trans>
            </h2>
            {history.length > 0 && clearConfirm.trigger}
          </div>
          <p className="text-xs text-muted-foreground">
            <Trans>
              {active.length} 个进行中 · {history.length} 个已结束
            </Trans>
          </p>

          {clearConfirm.panel}
          {clearAction.error && <WebErrorCard error={clearAction.error} className="text-xs" />}

          {total === 0 ? (
            <p className="text-xs text-muted-foreground">
              <Trans>还没有传输会话。</Trans>
            </p>
          ) : (
            <div className="flex min-h-0 flex-col gap-4 overflow-y-auto">
              {active.length > 0 && (
                <ul className="flex flex-col gap-1.5">
                  {active.map(renderRow)}
                </ul>
              )}
              {history.length > 0 && (
                <div className="border-t pt-3">
                  <p className="text-xs font-medium text-muted-foreground">
                    <Trans>最近完成</Trans>
                  </p>
                  <ul className="mt-2 flex flex-col gap-1.5">
                    {history.map(renderRow)}
                  </ul>
                </div>
              )}
            </div>
          )}
        </div>
        );
      }}
      detail={({ openList }) =>
        selected ? (
          <TransferDetailPanel
            openList={openList}
            projection={selected}
            progress={progress[selected.sessionId]}
            connection={connectionLabel(selected, connections)}
            ready={ready}
            resume={{
              pending: resumeAction.isPending(selected.sessionId),
              error: resumeAction.errorFor(selected.sessionId),
              run: () => resume(selected.sessionId),
            }}
            cancel={{
              pending: cancelAction.isPending(selected.sessionId),
              error: cancelAction.errorFor(selected.sessionId),
              run: () => cancel(selected.sessionId, selected.direction),
            }}
            remove={{
              pending: deleteAction.isPending(selected.sessionId),
              error: deleteAction.errorFor(selected.sessionId),
              run: () => remove(selected.sessionId),
            }}
          />
        ) : (
          <div className="flex flex-col gap-3 rounded-xl border bg-card p-6 shadow-xs">
            <div className="flex items-center gap-2">
              <OpenListButton openList={openList} label={t`打开传输会话列表`} />
              <h2 className="text-sm font-semibold text-foreground">
                <Trans>传输</Trans>
              </h2>
            </div>
            {/* 教学文案放详情侧，列表栏只说一行「这里是空的」——窄屏用户落在详情屏、
                列表收在抽屉里；两边都摆整套空态则是宽屏下同一句话说两遍。 */}
            <p className="text-xs text-muted-foreground">
              {total > 0 ? (
                <Trans>从左侧选一条会话，这里会显示逐文件进度与可用操作。</Trans>
              ) : (
                <Trans>还没有传输会话。到设备页点某台设备的「发送」即可开始一次传输。</Trans>
              )}
            </p>
          </div>
        )
      }
    />
  );
}

// memo：store 逐 key immutable 更新 projections/progress，未变动的会话保持原引用——
// 一个会话每秒十余次的进度事件因此只重渲染它自己那一行，而不是整张活动 + 历史列表。
//
// 列表行只承载「认出这一条 + 看它到哪了」：方向·对端、连接方式、阶段、总进度。
// 逐文件进度与动作归详情侧（`TransferDetailPanel`）——那些东西在一栏列表里塞不下，
// 塞进去就是此前那个手风琴：展开一条把其余全推到屏幕外。
const TransferActivityItem = memo(function TransferActivityItem({
  projection,
  progress: liveProgress,
  connection,
  selected,
  onSelect,
}: {
  projection: TransferProjection;
  progress?: TransferProgressEvent;
  connection: MessageDescriptor;
  selected: boolean;
  onSelect: (sessionId: string) => void;
}) {
  const { t } = useLingui();
  // `progress` 是**在途采样**：会话一进终态内核就不再下发，最后收到的那一帧会永远
  // 停在那儿。而下面每一处都让采样优先于 projection，于是终态被一个陈旧值盖住。
  //
  // projection 才是权威——它在终态已回填全量字节。2026-07-28 实测：16 MiB 传完、
  // `projection.transferredBytes` 已是 16777216，采样却停在 3932160，界面显示
  // 「已完成 · 23%」。
  const ended = projection.phase === "terminal";
  const progress = ended ? undefined : liveProgress;

  const percent = transferPercent(projection, progress);
  const bytesDone = progress?.transferredBytes ?? projection.transferredBytes;
  const totalBytes = progress?.totalBytes ?? projection.totalSize;

  return (
    <li>
      <button
        type="button"
        onClick={() => onSelect(projection.sessionId)}
        aria-current={selected ? "true" : undefined}
        className={cn(
          "w-full cursor-pointer rounded-lg border px-3 py-2.5 text-left transition-colors",
          selected
            ? "border-[var(--brand)]/40 bg-accent ring-1 ring-[var(--brand)]/20"
            : "hover:bg-accent",
        )}
      >
        <div className="flex items-center gap-2">
          <StatusDot
            colorClass={PHASE_META[projection.phase].dot}
            pulse={projection.phase === "active"}
          />
          <p className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
            {t(DIRECTION_LABEL[projection.direction])} · {projection.peerName}
          </p>
          <span className="shrink-0 text-[11px] text-muted-foreground">{t(phaseLabel(projection))}</span>
        </div>

        <div className="mt-2 flex items-center justify-between gap-3 text-[11px] text-muted-foreground">
          <span className="truncate">{t(connection)}</span>
          <span className="shrink-0 font-mono tabular-nums">
            {formatFileSize(bytesDone)} / {formatFileSize(totalBytes)} · {percent}%
          </span>
        </div>
        <ProgressBar percent={percent} className="mt-1.5" />
      </button>
    </li>
  );
});

/**
 * 详情侧 —— 选中会话的逐文件进度与可用操作。
 *
 * 速率与 ETA 只对进行中的会话有意义；终态显示「等待数据 · ETA 未知」是在报告一个不存在的等待。
 */
function TransferDetailPanel({
  openList,
  projection,
  progress: liveProgress,
  connection,
  ready,
  resume,
  cancel,
  remove,
}: {
  openList: (() => void) | null;
  projection: TransferProjection;
  progress?: TransferProgressEvent;
  connection: MessageDescriptor;
  ready: boolean;
  resume: ItemAction;
  cancel: ItemAction;
  remove: ItemAction;
}) {
  const { t } = useLingui();
  const ended = projection.phase === "terminal";
  const progress = ended ? undefined : liveProgress;
  const percent = transferPercent(projection, progress);
  const bytesDone = progress?.transferredBytes ?? projection.transferredBytes;
  const totalBytes = progress?.totalBytes ?? projection.totalSize;
  const files: TransferFileRow[] = progress?.files ?? projection.files;
  // 占位在这里给：共享的格式化函数算不出来时返回 null / 只收确定值，
  // 「等待数据」「未知」是要翻译的 UI 文案（见 `_lib/format.ts` 的说明）。
  const rate = formatTransferRate(progress?.speed) ?? t`等待数据`;
  const eta =
    progress?.eta != null && Number.isFinite(progress.eta) && progress.eta >= 0
      ? formatDuration(progress.eta)
      : t`未知`;

  return (
    <div className="flex flex-col gap-4 rounded-xl border bg-card p-4 shadow-xs sm:p-6">
      <div className="flex items-start gap-2">
        <OpenListButton openList={openList} label={t`打开传输会话列表`} />
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-2 text-sm font-medium text-foreground">
            <StatusDot
              colorClass={PHASE_META[projection.phase].dot}
              pulse={projection.phase === "active"}
            />
            <span className="truncate">
              {t(DIRECTION_LABEL[projection.direction])} · {projection.peerName}
            </span>
          </p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t(phaseLabel(projection))} · {t(connection)}
            {!ended && (
              <>
                {" · "}
                <Trans>
                  {rate} · ETA {eta}
                </Trans>
              </>
            )}
          </p>
          <p className="mt-1 font-mono text-[11px] break-all text-muted-foreground">
            {projection.sessionId}
          </p>
        </div>
      </div>

      <div>
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span className="font-mono tabular-nums">
            {formatFileSize(bytesDone)} / {formatFileSize(totalBytes)}
          </span>
          <span className="font-mono tabular-nums">{percent}%</span>
        </div>
        <ProgressBar percent={percent} className="mt-1.5" />
      </div>

      <ul className="flex flex-col gap-1.5">
        {files.map((file) => {
          const done = "transferred" in file ? file.transferred : file.transferredBytes;
          return (
            <li
              key={file.fileId}
              className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-lg border bg-background px-3 py-2 text-[11px]"
            >
              <span className="truncate text-foreground">{file.name}</span>
              <span className="font-mono tabular-nums text-muted-foreground">
                {formatFileSize(done)} / {formatFileSize(file.size)} · {calcPercent(done, file.size)}%
              </span>
            </li>
          );
        })}
      </ul>

      <TransferItemActions
        projection={projection}
        ready={ready}
        resume={resume}
        cancel={cancel}
        remove={remove}
      />
    </div>
  );
}

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
  const { t } = useLingui();
  const elapsed = elapsedSeconds(projection);

  return (
    <>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
        {/* 已用时长只对算得出来的会话有意义；算不出来（没有 startedAt）就什么都不说，
            而不是摆一个「已用 未知」。 */}
        <span>{elapsed === null ? null : <Trans>已用 {formatDuration(elapsed)}</Trans>}</span>
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
              {resume.pending ? <Trans>续传中</Trans> : <Trans>续传</Trans>}
            </button>
          )}
          {/* 判据用 isActiveSession——导航徽标与分组也用它，另写一份会在新增 phase 时对不上。 */}
          {isActiveSession(projection) && (
            <ConfirmAction
              icon={XCircle}
              label={t`取消`}
              pendingLabel={t`取消中`}
              confirmLabel={t`确认取消`}
              // 取消是不可逆的终态动作，却与「续传」并排——误点的代价不对称。
              warning={t`取消后无法恢复`}
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
              label={t`删除`}
              pendingLabel={t`删除中`}
              confirmLabel={t`确认删除`}
              // suspended 那条连断点一起没，代价比删一条普通记录大，得分开说。
              warning={
                projection.phase === "suspended"
                  ? t`断点信息将一并清除，无法再续传；已接收的文件仍在收件箱`
                  : t`只删这条记录，已接收的文件仍在收件箱`
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
