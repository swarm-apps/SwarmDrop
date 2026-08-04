"use client";

// #80 传输活动视图：projection 是生命周期主状态，progress 只补充实时速率/ETA/单文件进度。
//
// #93 选中态放 URL search param（`/app/transfer/?session=…`），刷新与分享后停在同一条会话上。
// **不能改用 `/app/transfer/[id]` 动态路由段**：docs 是 `output: "export"` 静态导出，动态段要
// `generateStaticParams` 预生成，而 sessionId 是运行时 UUID，永远预生成不出来。
// 详情就地展开而非另开页面：一次传输的信息量撑不起一整屏，展开足够，也省掉一次跳转。

import { ArrowDownToLine, ArrowUpFromLine, Check, Copy, Inbox, Pause, RotateCcw, Trash2, XCircle } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import {
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
import { RelativeTime } from "./relative-time";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import { useCopyToClipboard } from "../_lib/use-copy";
import {
  isActiveSession,
  isDeletableSession,
  isLostOnReload,
  isPausableSession,
  sessionEndedAt,
  sortByUpdatedDesc,
  transferSample,
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

/**
 * 详情侧逐文件行的默认展示条数，其余折在「显示全部」后面。
 *
 * 一次传一整个文件夹（几十上百个文件）是常态，而每一行现在都带自己的进度条，进行中的会话
 * 每秒还要重画一遍。全量渲染换不来可读性——超过十几行时用户已经在滚动里找不着自己关心的
 * 那个文件了，真要逐个核对的人才需要展开。
 */
const FILE_LIST_LIMIT = 12;

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

  const pauseAction = useKeyedAsyncAction();
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

  // 暂停：域层停 actor、把文件级进度落库、转 `suspended(LocalPaused)`（`recoverable = true`，
  // 所以按钮会立刻换成「续传」），并通知对端。
  //
  // **方向不自动判**，与下面的 `cancel` 同一理由——暂停也是有副作用的操作（停 actor、
  // 写状态、发帧），拿它当探针试方向会在第一条真失败时顺手对另一个方向也来一遍。
  //
  // 成功后同样**不动本地状态**：新阶段由内核经 projection 事件回流。
  const pause = useCallback(
    (sessionId: string, direction: TransferProjection["direction"]) => {
      const node = getNode();
      if (!node) return;
      void pauseAction.run(sessionId, () =>
        direction === "send" ? node.pause_send(sessionId) : node.pause_receive(sessionId),
      );
    },
    [pauseAction.run],
  );

  // 下面几个动作都先取一次 node；取不到就静默返回——按钮已由 `ready` 禁用，这里只是兜底。
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
            pause={{
              pending: pauseAction.isPending(selected.sessionId),
              error: pauseAction.errorFor(selected.sessionId),
              run: () => pause(selected.sessionId, selected.direction),
            }}
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
// 列表行只承载「认出这一条 + 看它到哪了」。**认出靠文件名，不是靠对端名**：一条会话在用户
// 心里叫「我发给他的那个安装包」，而不是「发送 · MacBook」——同一台设备来回传几次之后，
// 只写方向与对端的三行长得一模一样，选中哪条全靠猜。
//
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
  // 「终态以 projection 为准」的取舍收在 `transferSample` 里（见那里的说明）。
  const { live, done, total, percent } = transferSample(projection, liveProgress);
  const DirectionIcon = projection.direction === "send" ? ArrowUpFromLine : ArrowDownToLine;
  // 速率只在真的在传时才有意义。其余阶段（等待接受 / 已中断 / 已结束）给时间——
  // 「什么时候的事」在那些阶段正是用户要问的（同桌面 `-session-row.tsx` 的右列）。
  const rate = projection.phase === "active" ? formatTransferRate(live?.speed) : null;

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
          <SessionTitle files={projection.files} fallback={projection.peerName} />
          <span className="shrink-0 text-[11px] text-muted-foreground">{t(phaseLabel(projection))}</span>
        </div>

        <div className="mt-1 flex items-center justify-between gap-3 text-[11px] text-muted-foreground">
          <span className="flex min-w-0 items-center gap-1">
            {/* 方向此前是「发送 / 接收」两个字，与对端名抢同一行的宽度。图标表达同一件事
                却只占一个字符；文字留在 `aria-label` 里，读屏拿到的信息不减。 */}
            <DirectionIcon
              className="size-3 shrink-0"
              role="img"
              aria-label={t(DIRECTION_LABEL[projection.direction])}
            />
            <span className="truncate">{projection.peerName}</span>
          </span>
          <span className="shrink-0 font-mono tabular-nums">
            {formatFileSize(done)} / {formatFileSize(total)}
          </span>
        </div>

        <ProgressBar percent={percent} className="mt-1.5" label={t`传输进度`} />

        <div className="mt-1 flex items-center justify-between gap-3 text-[11px] text-muted-foreground">
          <span className="truncate">{t(connection)}</span>
          {/* 同一个位置轮流放两样东西，而不是各占一行：正在传时问「多快」，其余阶段问
              「什么时候的事」——两个问题不会同时成立，所以也不该同时占版面。 */}
          <span className="flex shrink-0 items-center gap-1">
            {rate ? (
              <span className="font-mono tabular-nums">{rate}</span>
            ) : (
              <RelativeTime timestamp={sessionEndedAt(projection)} />
            )}
            <span aria-hidden>·</span>
            <span className="font-mono tabular-nums">{percent}%</span>
          </span>
        </div>
      </button>
    </li>
  );
});

/**
 * 会话标题：首个文件名 + 「还有几个」的计数徽标。
 *
 * 计数**不并进被截断的那段文字**（「a.zip 等 3 个文件」在窄列里必然被切掉尾巴，而尾巴才是
 * 计数）——它自己占一个 `shrink-0` 的位，永远看得见。
 *
 * `files` 为空只可能出现在异常投影上，那时回落到对端名，至少还认得出是跟谁的会话。
 */
function SessionTitle({ files, fallback }: { files: TransferProjection["files"]; fallback: string }) {
  const { t } = useLingui();
  const first = files[0];
  const rest = files.length - 1;

  return (
    <p className="flex min-w-0 flex-1 items-center gap-1.5 text-xs font-medium text-foreground">
      <span className="truncate" title={first?.name ?? fallback}>
        {first?.name ?? fallback}
      </span>
      {rest > 0 && (
        <span
          className="shrink-0 rounded-full bg-muted px-1.5 text-[10px] font-normal text-muted-foreground"
          title={t`共 ${files.length} 个文件`}
        >
          +{rest}
        </span>
      )}
    </p>
  );
}

/**
 * 详情侧 —— 选中会话的逐文件进度与可用操作。
 *
 * 速率与 ETA 只对**正在传**的会话有意义。此前的判据是「非终态」，于是「等待对方接受」与
 * 「已中断」两个阶段也会摆出一行「等待数据 · ETA 未知」——那是在报告一个不存在的等待，
 * 而这两个阶段恰恰是用户最想知道「到底卡在哪」的时候。现在这两档不给指标，给的是阶段本身
 * 的说明（`phaseLabel` 已区分出「对方暂停 / 连接中断 / 对方离线」）。
 */
function TransferDetailPanel({
  openList,
  projection,
  progress: liveProgress,
  connection,
  ready,
  pause,
  resume,
  cancel,
  remove,
}: {
  openList: (() => void) | null;
  projection: TransferProjection;
  progress?: TransferProgressEvent;
  connection: MessageDescriptor;
  ready: boolean;
  pause: ItemAction;
  resume: ItemAction;
  cancel: ItemAction;
  remove: ItemAction;
}) {
  const { t } = useLingui();
  const { live, done, total, percent } = transferSample(projection, liveProgress);
  const files: TransferFileRow[] = live?.files ?? projection.files;
  const DirectionIcon = projection.direction === "send" ? ArrowUpFromLine : ArrowDownToLine;

  return (
    <div className="flex flex-col gap-4 rounded-xl border bg-card p-4 shadow-xs sm:p-6">
      <div className="flex items-start gap-2">
        <OpenListButton openList={openList} label={t`打开传输会话列表`} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-sm font-medium text-foreground">
            <StatusDot
              colorClass={PHASE_META[projection.phase].dot}
              pulse={projection.phase === "active"}
            />
            <SessionTitle files={projection.files} fallback={projection.peerName} />
          </div>
          <p className="mt-0.5 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
            <span className="flex items-center gap-1">
              <DirectionIcon
                className="size-3"
                role="img"
                aria-label={t(DIRECTION_LABEL[projection.direction])}
              />
              {projection.peerName}
            </span>
            <span aria-hidden>·</span>
            <span>{t(phaseLabel(projection))}</span>
            <span aria-hidden>·</span>
            <span>{t(connection)}</span>
          </p>
        </div>
      </div>

      <div>
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span className="font-mono tabular-nums">
            {formatFileSize(done)} / {formatFileSize(total)}
          </span>
          <span className="font-mono tabular-nums">{percent}%</span>
        </div>
        <ProgressBar percent={percent} className="mt-1.5" label={t`传输进度`} />
      </div>

      <TransferMetrics projection={projection} progress={live} />

      {/* 「本页限定」要在用户**做决定之前**就说，所以判据是 `isLostOnReload` 而不是
          `phase === "suspended"`：等暂停完再说就晚了——「待会儿再继续」的预期在点下那一刻
          就已经形成，而传输中刷新同样整条丢失（非终态发送会话一律不落库，见
          `crates/web/src/store.rs` 的落库范围表）。接收方向没有这个限制，半成品在 OPFS 里
          续得上，故本提示天然不出现在那一侧。 */}
      {isLostOnReload(projection) && (
        <p className="rounded-lg border bg-muted/40 px-3 py-2 text-[11px] text-muted-foreground">
          <Trans>
            这条发送只能在本页完成：刷新或关闭标签页后，浏览器读不回你选过的文件，会话与进度会一并消失。暂停后也只能在本页续传。
          </Trans>
        </p>
      )}

      {/* key 绑会话：切到另一条会话时详情面板不卸载，展开态会跟着漂过去——上一条展开了
          四十行，下一条一进来也是全展开的。 */}
      <TransferFileList key={projection.sessionId} files={files} />

      <TransferItemActions
        projection={projection}
        ready={ready}
        pause={pause}
        resume={resume}
        cancel={cancel}
        remove={remove}
      />

      <SessionIdRow key={projection.sessionId} sessionId={projection.sessionId} />
    </div>
  );
}

/**
 * 指标区。**只在数据真的存在的阶段出现**：
 *
 * - `active`：速率 + 剩余时间——这两个数只有内核在下发进度事件时才有。
 * - `terminal`：用时 + 结束时刻——传完之后用户关心的是「花了多久」「什么时候的事」。
 * - 其余（等待接受 / 已中断）：整块不渲染，由阶段文案自己说话。
 *
 * 单条指标算不出来时同样整格省掉，而不是摆一个「未知」——一格「速率：未知」传达的信息
 * 与不摆这一格完全相同，却多占一份视觉重量。
 */
function TransferMetrics({
  projection,
  progress,
}: {
  projection: TransferProjection;
  progress?: TransferProgressEvent;
}) {
  const { t } = useLingui();
  const metrics: Array<{ label: string; value: string }> = [];

  if (projection.phase === "active") {
    const rate = formatTransferRate(progress?.speed);
    if (rate) metrics.push({ label: t`速率`, value: rate });
    if (progress?.eta != null && Number.isFinite(progress.eta) && progress.eta >= 0) {
      metrics.push({ label: t`剩余`, value: formatDuration(progress.eta) });
    }
  } else if (projection.phase === "terminal") {
    const elapsed = elapsedSeconds(projection);
    if (elapsed !== null) metrics.push({ label: t`用时`, value: formatDuration(elapsed) });
  }

  const endedAt = projection.phase === "terminal" ? sessionEndedAt(projection) : null;
  if (metrics.length === 0 && endedAt === null) return null;

  return (
    <dl className="flex flex-wrap gap-x-6 gap-y-2">
      {metrics.map((metric) => (
        <div key={metric.label} className="flex flex-col">
          <dt className="text-[11px] text-muted-foreground">{metric.label}</dt>
          <dd className="font-mono text-xs tabular-nums text-foreground">{metric.value}</dd>
        </div>
      ))}
      {endedAt !== null && (
        <div className="flex flex-col">
          <dt className="text-[11px] text-muted-foreground">
            <Trans>结束于</Trans>
          </dt>
          <dd className="text-xs text-foreground">
            <RelativeTime timestamp={endedAt} />
          </dd>
        </div>
      )}
    </dl>
  );
}

/**
 * 逐文件清单。
 *
 * 每一行带自己的进度条与状态：`FileProgressInfo.status`（pending / transferring / completed）
 * 此前一直在事件里躺着没人读——没有它，一个 40 个文件的会话在传输过程中每一行长得一模一样，
 * 看不出「现在在传哪个」。projection 侧没有这个字段（那是持久化投影，不含在途状态），
 * 故按百分比推断，两条路径归一到同一个渲染。
 *
 * 超过 `FILE_LIST_LIMIT` 折起来：见该常量的说明。
 */
function TransferFileList({ files }: { files: TransferFileRow[] }) {
  const { t } = useLingui();
  const [expanded, setExpanded] = useState(false);
  const hidden = files.length - FILE_LIST_LIMIT;
  const shown = expanded ? files : files.slice(0, FILE_LIST_LIMIT);

  return (
    <div className="flex flex-col gap-1.5">
      <ul className="flex flex-col gap-1.5">
        {shown.map((file) => {
          const done = "transferred" in file ? file.transferred : file.transferredBytes;
          const percent = calcPercent(done, file.size);
          const status = "status" in file ? file.status : percent >= 100 ? "completed" : "pending";
          return (
            <li key={file.fileId} className="rounded-lg border bg-background px-3 py-2 text-[11px]">
              <div className="flex items-center justify-between gap-3">
                <span className="flex min-w-0 items-center gap-1.5">
                  {status === "completed" ? (
                    // 完成态给对勾而不是「100%」：一眼扫下去，形状比数字快。
                    <Check
                      className="size-3 shrink-0 text-emerald-600 dark:text-emerald-400"
                      role="img"
                      aria-label={t`已完成`}
                    />
                  ) : (
                    <StatusDot
                      colorClass={status === "transferring" ? "bg-[var(--brand-solid)]" : "bg-muted-foreground/40"}
                      pulse={status === "transferring"}
                      label={status === "transferring" ? t`传输中` : t`等待中`}
                    />
                  )}
                  <span
                    className={cn("truncate", status === "pending" ? "text-muted-foreground" : "text-foreground")}
                    title={file.name}
                  >
                    {file.name}
                  </span>
                </span>
                <span className="shrink-0 font-mono tabular-nums text-muted-foreground">
                  {formatFileSize(done)} / {formatFileSize(file.size)}
                </span>
              </div>
              {/* 只有真的传了一部分才画进度条：已完成的满格条把对勾又说了一遍，还没开始的
                  空轨道则是一整行「零信息」——一次几十个文件的会话里，那是几十条空轨道。 */}
              {status !== "completed" && percent > 0 && (
                <ProgressBar percent={percent} className="mt-1.5" label={t`${file.name} 的进度`} />
              )}
            </li>
          );
        })}
      </ul>
      {hidden > 0 && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          className="self-start rounded-lg px-1 text-[11px] text-muted-foreground underline underline-offset-2 hover:text-foreground"
        >
          {expanded ? <Trans>收起文件</Trans> : <Trans>显示全部 {files.length} 个文件</Trans>}
        </button>
      )}
    </div>
  );
}

/**
 * 会话 ID 行。
 *
 * 它此前顶在标题下方、`break-all` 铺满三行——一串对用户毫无意义的 UUID 占据了详情侧最贵的
 * 位置。但它也不能删：报 issue、翻日志、对事件流都靠它，那正是 DESIGN.md 对 multiaddr 的
 * 同一条态度（「对普通用户是噪声，对排查的人是全部答案」，所以降级而不是丢弃）。
 *
 * 复制态住在本组件里，换代 `key` 由调用方挂在**本组件**上——挂在里面那个 `<button>` 的
 * DOM 节点上什么也重置不了（知识库「复制态的换代 key」）。
 */
function SessionIdRow({ sessionId }: { sessionId: string }) {
  const { t } = useLingui();
  const { state, copy } = useCopyToClipboard();

  return (
    <div className="flex items-center gap-1.5 border-t pt-3 text-[11px] text-muted-foreground">
      <span className="truncate font-mono" title={sessionId}>
        {sessionId}
      </span>
      <button
        type="button"
        onClick={() => void copy(sessionId)}
        // 成功只换了个图标——读屏用户看不见图标，所以可访问名要跟着换，否则那次点击对他们
        // 而言没有任何反馈。
        aria-label={state === "copied" ? t`已复制` : t`复制会话 ID`}
        title={t`复制会话 ID`}
        className="-m-2 flex size-9 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground"
      >
        {state === "copied" ? (
          <Check className="size-3 text-emerald-600 dark:text-emerald-400" aria-hidden />
        ) : (
          <Copy className="size-3" aria-hidden />
        )}
      </button>
      {state === "failed" && (
        <span className="shrink-0 text-amber-600 dark:text-amber-400">
          <Trans>复制失败</Trans>
        </span>
      )}
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
  pause,
  resume,
  cancel,
  remove,
}: {
  projection: TransferProjection;
  /** 节点未就绪时所有动作都点不动——共用一个前置条件，不必各传一份。 */
  ready: boolean;
  pause: ItemAction;
  resume: ItemAction;
  cancel: ItemAction;
  remove: ItemAction;
}) {
  const { t } = useLingui();

  return (
    <>
      {/* 已用时长归指标区（`TransferMetrics`）——它此前挤在动作行的左端，与右端那排按钮
          是两件事，同一行里读起来像是某个按钮的说明。 */}
      <div className="flex flex-wrap items-center justify-end gap-2 text-xs text-muted-foreground">
        <div className="flex flex-wrap items-center gap-2">
          {/* 传输页是「过程」、收件箱是「结果」（分工见 inbox/page.tsx），此前两者是两座孤岛：
              一次接收在两处各出现一次，却没有任何一条边把它们连起来。 */}
          {projection.direction === "receive" && (
            <InboxItemLink sessionId={projection.sessionId} phase={projection.phase} ready={ready} />
          )}
          {/* 暂停与续传是同一个开关的两半，判据互斥（`active` ↔ `suspended`），所以同一位置
              永远只出现一个。两者都可逆，都不设二次确认——只有不可逆的那两个才拦。 */}
          {isPausableSession(projection) && (
            <button
              type="button"
              onClick={pause.run}
              disabled={!ready || pause.pending}
              className={INLINE_ACTION_CLASS}
            >
              <Pause className="size-3" aria-hidden="true" />
              {pause.pending ? <Trans>暂停中</Trans> : <Trans>暂停</Trans>}
            </button>
          )}
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
      {pause.error && <WebErrorCard error={pause.error} className="mt-2 text-xs" />}
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
      <Trans>查看收到的文件</Trans>
    </Link>
  );
}
