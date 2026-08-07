"use client";

// #80 传输活动视图：projection 是生命周期主状态，progress 只补充实时速率/ETA/单文件进度。
//
// #93 选中态放 URL search param（`/app/transfer/?session=…`），刷新与分享后停在同一条会话上。
// **不能改用 `/app/transfer/[id]` 动态路由段**：docs 是 `output: "export"` 静态导出，动态段要
// `generateStaticParams` 预生成，而 sessionId 是运行时 UUID，永远预生成不出来。
// 详情就地展开而非另开页面：一次传输的信息量撑不起一整屏，展开足够，也省掉一次跳转。

import {
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowUpFromLine,
  MonitorSmartphone,
  Trash2,
} from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import {
  Suspense,
  memo,
  useCallback,
  useMemo,
  useRef,
  useState,
} from "react";
import { Button } from "@/components/ui/button";
import { useConfirmAction } from "./confirm-action";
import { CenteredEmptyState, PanelSkeleton, RailEmptyHint } from "./empty-state";
import { NodeNotReadyState } from "./node-not-ready-state";
import { PanelFallback } from "./panel-fallback";
import { ProgressBar } from "./progress-bar";
import { RelativeTime } from "./relative-time";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import {
  sessionEndedAt,
  sortByUpdatedDesc,
  transferSample,
} from "../_lib/format";
import type { MessageDescriptor } from "@lingui/core";
// 标签表与纯函数住在 `transfer-labels.ts`——它们不渲染任何东西，列表行与详情侧共用同一份。
import {
  DIRECTION_LABEL,
  FILTER_LABEL,
  PHASE_META,
  connectionByPeer,
  connectionLabel,
  groupSessions,
  phaseLabel,
  type SessionFilter,
} from "./transfer-labels";
import { Trans, useLingui } from "@lingui/react/macro";
import {
  formatFileSize,
  formatTransferRate,
} from "@swarmdrop/shared-view";
import { cn } from "@/lib/cn";
import { PANEL_SURFACE, selectedRowClass } from "./section";
import { MasterDetail, OpenListButton } from "./master-detail";
// 详情侧整块住在 `transfer-detail.tsx`——本文件只做编排（主从布局、筛选、动作分发）与列表行。
import { SessionTitle } from "./session-title";
import { TransferDetailPanel } from "./transfer-detail";
import { NAV, PARAM, transferSessionHref } from "../_lib/nav";
import { peerLabel } from "../_lib/device-presentation";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { type TransferProgressEvent, type TransferProjection } from "../_lib/view-types";

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

  const [filter, setFilter] = useState<SessionFilter>("all");

  // 排序只依赖 projections；分组才依赖筛选。分两个 memo，换筛选不会连排序一起重跑。
  const sorted = useMemo(() => sortByUpdatedDesc(Object.values(projections)), [projections]);
  const { active, history, total } = useMemo(() => groupSessions(sorted, filter), [sorted, filter]);
  /** 会话总数（不受筛选影响）——空态要靠它区分「一条都没有」与「这一档是空的」。 */
  const grandTotal = sorted.length;
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

          {/* 筛选。只在有会话时出现——一条都没有时，四个筛选档全是空的，纯占版面。
              桌面 `_app/transfer/index.lazy.tsx` 有同样的四档，这里此前完全没有：
              历史硬截 8 条 + 无筛选，第 9 条起在界面上根本够不着。 */}
          {grandTotal > 0 && (
            <div role="group" aria-label={t`按状态筛选会话`} className="flex flex-wrap gap-1.5">
              {(["all", "active", "recoverable", "ended"] as const).map((key) => (
                <button
                  key={key}
                  type="button"
                  aria-pressed={filter === key}
                  onClick={() => setFilter(key)}
                  className={cn(
                    "focus-ring min-h-11 rounded-full border px-3 text-[11px] font-medium transition-colors sm:min-h-7",
                    filter === key
                      ? "border-transparent bg-primary text-primary-foreground"
                      : "text-muted-foreground hover:bg-accent",
                  )}
                >
                  {t(FILTER_LABEL[key])}
                </button>
              ))}
            </div>
          )}

          {clearConfirm.panel}
          {clearAction.error && <WebErrorCard error={clearAction.error} className="text-xs" />}

          {/* 「节点还没起来」不等于「你没有会话」：历史要等 wasm 加载完才回补，
              期间断言「还没有传输会话」是在说一件当时并不成立的事。 */}
          {grandTotal === 0 && !ready ? (
            <PanelSkeleton rows={3} />
          ) : grandTotal === 0 ? (
            <RailEmptyHint>
              <Trans>还没有传输会话。</Trans>
            </RailEmptyHint>
          ) : total === 0 ? (
            // 「这一档是空的」与「一条会话都没有」是两件事。合成一句「还没有传输会话」
            // 会让刚点了「可恢复」的用户以为自己的历史没了。
            <RailEmptyHint>
              <Trans>这个筛选下没有会话。</Trans>
            </RailEmptyHint>
          ) : (
            <div className="flex min-h-0 flex-col gap-4 overflow-y-auto">
              {active.length > 0 && (
                <ul className="flex flex-col gap-1.5">
                  {active.map(renderRow)}
                </ul>
              )}
              {history.length > 0 && (
                <div className={cn(active.length > 0 && "border-t pt-3")}>
                  {active.length > 0 && (
                    <p className="text-xs font-medium text-muted-foreground">
                      <Trans>已结束</Trans>
                    </p>
                  )}
                  <ul className={cn("flex flex-col gap-1.5", active.length > 0 && "mt-2")}>
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
          <div className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", PANEL_SURFACE)}>
            <div className="flex shrink-0 items-center gap-2 border-b px-4 py-3">
              <OpenListButton openList={openList} label={t`打开传输会话列表`} />
              <h2 className="text-sm font-semibold text-foreground">
                <Trans>传输</Trans>
              </h2>
            </div>
            {/* 教学文案放详情侧，列表栏只说一行「这里是空的」——窄屏用户落在详情屏、
                列表收在抽屉里；两边都摆整套空态则是宽屏下同一句话说两遍。

                判据读 `grandTotal`（不受筛选影响）而不是 `total`：详情侧问的是「这个应用里
                有没有会话」。用筛选后的数会让点一下「可恢复」就在右边说「还没有传输」，
                而左边的历史明明还在。 */}
            {!ready && grandTotal === 0 ? (
              <NodeNotReadyState
                description={<Trans>节点起来后，进行中与已结束的会话会出现在这里。</Trans>}
              />
            ) : grandTotal > 0 ? (
              <CenteredEmptyState
                icon={ArrowLeftRight}
                title={<Trans>选一条会话</Trans>}
                description={<Trans>选中后这里会显示逐文件进度、链路证据与可用操作。</Trans>}
              />
            ) : (
              <CenteredEmptyState
                icon={ArrowLeftRight}
                title={<Trans>还没有传输</Trans>}
                description={
                  <Trans>到设备页点某台在线设备的「发送」，传输开始后会实时出现在这里。</Trans>
                }
                action={
                  <Button asChild size="sm">
                    <Link href={NAV.devices.href}>
                      <MonitorSmartphone className="size-4" aria-hidden />
                      <Trans>去设备页</Trans>
                    </Link>
                  </Button>
                }
              />
            )}
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
  const peer = peerLabel(projection.peerName, projection.peerId);

  return (
    <li>
      <button
        type="button"
        onClick={() => onSelect(projection.sessionId)}
        aria-current={selected ? "true" : undefined}
        className={cn(
          "w-full cursor-pointer rounded-lg border px-3 py-2.5 text-left transition-colors",
          selectedRowClass(selected),
        )}
      >
        <div className="flex items-center gap-2">
          <StatusDot
            colorClass={PHASE_META[projection.phase].dot}
            pulse={projection.phase === "active"}
          />
          <SessionTitle files={projection.files} fallback={peer} />
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
            <span className="truncate">{peer}</span>
          </span>
          <span className="shrink-0 font-mono tabular-nums">
            {formatFileSize(done)} / {formatFileSize(total)}
          </span>
        </div>

        {/* 整行是个 `<button>`，而 button 的后代角色会被辅助技术整个丢弃
            （ARIA Children Presentational: True）。所以这里必须走装饰模式——
            进度由按钮自己的可访问名承担，下面那行的百分比数字就在名字里。
            详情侧（非按钮内）那条才是真的 `role="progressbar"`。 */}
        <ProgressBar percent={percent} className="mt-1.5" label={null} />

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

