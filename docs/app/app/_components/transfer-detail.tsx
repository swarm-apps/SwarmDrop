"use client";

// 传输**详情侧**：一条会话展开后的全部内容——概要头、指标、逐文件进度、会话 ID、可用动作，
// 以及「去收件箱看收到的文件」那条反查链接。
//
// 从 `transfer-activity-panel.tsx` 抽出来的。那个文件曾经在一处装下 8 个组件，其中
// `InboxItemLink` 甚至会从传输面板里发起一次**收件箱**的异步反查——两个域挤在同一个文件里，
// 谁都看不出边界在哪。现在的分工是：
//
//   transfer-activity-panel.tsx  编排（主从布局、筛选、动作分发）+ 列表行
//   transfer-detail.tsx          详情侧（本文件）
//   transfer-labels.ts           标签表与纯函数，两边共用
//
// 动作的**执行**仍归编排层：本文件收到的是已经绑好 sessionId 的 `ItemAction`，
// 只负责渲染它的 pending / error / 触发。哪个方向该调 `pause_send` 还是 `pause_receive`
// 这类判断留在编排层，因为那里才有 projection 全貌。

import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Check,
  Copy,
  Inbox,
  Pause,
  RotateCcw,
  Trash2,
  XCircle,
} from "lucide-react";
import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import type { MessageDescriptor } from "@lingui/core";
import {
  formatDuration,
  formatFileSize,
  formatTransferRate,
} from "@swarmdrop/shared-view";
import { FileBrowser } from "@swarmdrop/file-browser";
import { cn } from "@/lib/cn";
import { PANEL_SURFACE, fileSectionHeightClass } from "./section";
import { ConfirmAction, INLINE_ACTION_CLASS } from "./confirm-action";
import { OpenListButton } from "./master-detail";
import { ProgressBar } from "./progress-bar";
import { RelativeTime } from "./relative-time";
import { SessionTitle } from "./session-title";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import {
  DIRECTION_LABEL,
  PHASE_META,
  elapsedSeconds,
  phaseLabel,
} from "./transfer-labels";
import {
  isActiveSession,
  isCompletedSession,
  isDeletableSession,
  isLostOnReload,
  isPausableSession,
  sessionEndedAt,
  transferSample,
} from "../_lib/format";
import { peerLabel } from "../_lib/device-presentation";
import { itemsFromProjection } from "../_lib/file-browser-adapters";
import { inboxItemHref } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { preferencesActions, usePreferences } from "../_lib/preferences-store";
import { useCopyToClipboard } from "../_lib/use-copy";
import { failureCodeLabel } from "../_lib/view-types";
import type {
  ItemAction,
  TransferProgressEvent,
  TransferProjection,
} from "../_lib/view-types";

/**
 * 详情侧 —— 选中会话的逐文件进度与可用操作。
 *
 * 速率与 ETA 只对**正在传**的会话有意义。此前的判据是「非终态」，于是「等待对方接受」与
 * 「已中断」两个阶段也会摆出一行「等待数据 · ETA 未知」——那是在报告一个不存在的等待，
 * 而这两个阶段恰恰是用户最想知道「到底卡在哪」的时候。现在这两档不给指标，给的是阶段本身
 * 的说明（`phaseLabel` 已区分出「对方暂停 / 连接中断 / 对方离线」）。
 */
export function TransferDetailPanel({
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
  /** `null` = 连接方式查不到（历史会话的常态），此时摘要行里整段不渲染。 */
  connection: MessageDescriptor | null;
  ready: boolean;
  pause: ItemAction;
  resume: ItemAction;
  cancel: ItemAction;
  remove: ItemAction;
}) {
  const { t } = useLingui();
  // `transferSample` 仍然管**会话级**的字节数与百分比（它那条「终态一律以 projection 为准」
  // 的纪律对整条进度条依然成立）。逐文件那一路已经不走它了——见下面 `fileItems` 的说明。
  const { live, done, total, percent } = transferSample(projection, liveProgress);
  const DirectionIcon = projection.direction === "send" ? ArrowUpFromLine : ArrowDownToLine;
  const peer = peerLabel(projection.peerName, projection.peerId);
  // 判据与列表行同源（`isCompletedSession` 的注释里写了为什么不能用 phase 或 percent 代替）。
  const completed = isCompletedSession(projection);

  return (
    // 详情自己是滚动容器（`min-h-0 flex-1` + `overflow-y-auto`）：宽屏下滚详情不会带走
    // 左边的列表，窄屏下滚它也不会把页头顶走。面板级圆角走 18px 词汇，与控件的 8px 分开。
    //
    // 间距按 Layout Density Contract 分层（组内 8 / 面板内分区 16 / 面板内边距 20），
    // 而不是此前一路 `gap-4` 平铺：
    // 这一屏其实只有三段——**这条会话是什么**（标题 + 摘要 + 进度 + 指标）、**里面有哪些
    // 文件**、**能对它做什么**（动作 + 会话 ID）。六块等距排开时三段的边界无从读出，
    // 页面就成了一列等重的盒子（正是那份契约点名的失败样式）。
    <div
      className={cn(
        "flex min-h-0 flex-1 flex-col gap-[var(--space-in-panel)] overflow-y-auto p-[var(--space-panel)] sm:p-6",
        PANEL_SURFACE,
      )}
    >
      {/* 第一段：概况。四块贴着走（8px），因为它们讲的是同一件事。 */}
      <div className="flex flex-col gap-[var(--space-in-group)]">
        <div className="flex items-start gap-2">
          <OpenListButton openList={openList} label={t`打开传输会话列表`} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2 text-sm font-medium text-foreground">
              <StatusDot
                colorClass={PHASE_META[projection.phase].dot}
                pulse={projection.phase === "active"}
              />
              <SessionTitle files={projection.files} fallback={peer} />
            </div>
            <p className="mt-0.5 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
              <span className="flex items-center gap-1">
                <DirectionIcon
                  className="size-3"
                  role="img"
                  aria-label={t(DIRECTION_LABEL[projection.direction])}
                />
                {peer}
              </span>
              <span aria-hidden>·</span>
              <span>{t(phaseLabel(projection))}</span>
              {/* 连接方式查不到就整段省掉，连同那个分隔点——「连接类型未知」在每一条历史
                  会话上都成立，占着摘要行三分之一却不回答任何问题。 */}
              {connection && (
                <>
                  <span aria-hidden>·</span>
                  <span>{t(connection)}</span>
                </>
              )}
              {/* 传完的会话没有进度可言，总量就并进摘要行——单摆一行「9.3 KB」会让人
                  以为它是某个还没长出来的区块的标题。 */}
              {completed && (
                <>
                  <span aria-hidden>·</span>
                  <span className="font-mono tabular-nums">{formatFileSize(total)}</span>
                </>
              )}
            </p>
          </div>
        </div>

        {/* 传完的会话整块退场：满格进度条与 `9.3 KB / 9.3 KB` 说的都是上面那行「已完成」
            已经说过的话，总量已并进摘要行。断掉的终态（取消 / 失败）反而要留着——
            那时候「传到哪儿断的」正是唯一有用的信息。 */}
        {!completed && (
          <div>
            <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
              <span className="font-mono tabular-nums">
                {formatFileSize(done)} / {formatFileSize(total)}
              </span>
              <span className="font-mono tabular-nums">{percent}%</span>
            </div>
            <ProgressBar percent={percent} className="mt-1.5" label={t`传输进度`} />
          </div>
        )}

        <TransferMetrics projection={projection} progress={live} />

        {/* 「本页限定」要在用户**做决定之前**就说，所以判据是 `isLostOnReload` 而不是
            `phase === "suspended"`：等暂停完再说就晚了——「待会儿再继续」的预期在点下那一刻
            就已经形成，而传输中刷新同样整条丢失（非终态发送会话一律不落库，见
            `crates/web/src/store.rs` 的落库范围表）。接收方向没有这个限制，半成品在 OPFS 里
            续得上，故本提示天然不出现在那一侧。 */}
        {isLostOnReload(projection) && (
          <p className="mt-1 rounded-lg border bg-muted/40 px-3 py-2 text-[11px] text-muted-foreground">
            <Trans>
              这条发送只能在本页完成：刷新或关闭标签页后，浏览器读不回你选过的文件，会话与进度会一并消失。暂停后也只能在本页续传。
            </Trans>
          </p>
        )}
      </div>

      {/* 第二段：这条会话里有哪些文件。

          key 绑会话：切到另一条会话时详情面板不卸载，树的展开态与视图内部状态会跟着漂过去
          ——上一条展开了四十行，下一条一进来也是全展开的。

          ⚠️ **前缀不能省。** 下面的 `SessionIdRow` 也按会话换代，两者是同一个父下的兄弟；
          裸用 `projection.sessionId` 会让它们共用同一个 key，而 React 协调数组 children 时
          用的是一张 `key → fiber` 的 Map——后写的那个会把先写的挤掉，收尾只删 Map 里剩下的，
          于是被挤掉的这一块**永远不卸载**：每切一次会话，详情里就多堆一份旧的文件清单。
          生产构建下 React 不打「same key」警告，只能靠这条注释与前缀守。 */}
      <SessionFileSection
        key={`files-${projection.sessionId}`}
        projection={projection}
        progress={liveProgress}
      />

      {/* 第三段：能对它做什么。 */}
      <TransferItemActions
        projection={projection}
        ready={ready}
        pause={pause}
        resume={resume}
        cancel={cancel}
        remove={remove}
      />

      {/* 页脚：排查时才用得上的那串 ID。它自带一条 hairline——这是面板里唯一保留分隔线的
          地方，因为它要说的不是「新的一段」而是「以下不是给你看的」。
          前缀同上：与 `SessionFileSection` 是兄弟，key 不能撞。 */}
      <SessionIdRow key={`sid-${projection.sessionId}`} sessionId={projection.sessionId} />
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
 * 逐文件清单 —— 三端共用的 `FileBrowser`（树形 / 网格），与桌面 `SessionFileSection` 同构。
 *
 * ## 取数根治：投影是骨架，进度只是覆盖层
 *
 * 这里此前写的是 `live?.files ?? projection.files`——**二选一**。于是同一份数据有两种形状
 * （`FileProgressInfo.transferred` vs `TransferProjectionFile.transferredBytes`），渲染点要靠
 * `"transferred" in file` 现场嗅探；更糟的是行的**身份与数量**在两种形状下由不同的东西决定，
 * 而进度域是按 sessionId 常驻的，切换会话那一瞬取到的可能是另一条会话的采样。
 *
 * 现在两者都交给 `itemsFromProjection`：行永远来自 projection，progress 只按 `fileId` 覆盖
 * 「传了多少」与「什么状态」；终态忽略陈旧进度的判定也收在 L1 里，不再依赖调用点自觉。
 * 那条不变量有回归测试钉着（`packages/shared-view/src/file-browser/adapters.test.ts`）。
 *
 * 顺带解决的还有两件事：目录层级（此前是一列扁平文件名，一次传整个文件夹时读不出结构）、
 * 以及「超过 12 条折起来」那个截断——树形视图是虚拟滚动的，几百个文件也不必藏。
 */
function SessionFileSection({
  projection,
  progress,
}: {
  projection: TransferProjection;
  progress?: TransferProgressEvent;
}) {
  const view = usePreferences((s) => s.fileBrowserViews.transfer);
  const items = useMemo(
    () => itemsFromProjection(projection, progress),
    [projection, progress],
  );

  return (
    <FileBrowser
      items={items}
      title={<Trans>文件</Trans>}
      view={view}
      onViewChange={(nextView) => preferencesActions.setFileBrowserView("transfer", nextView)}
      // 详情侧自己是滚动容器，本区块按内容定高（`flex-none` 覆盖组件默认的 `flex-1`——
      // 那个默认是给「文件浏览器就是整屏主体」的布局用的，这里它只是详情里的一节）。
      // 高度给**上限**不给下限，两档按视图分——理由都在 `fileSectionHeightClass` 上。
      className="flex-none"
      contentClassName={fileSectionHeightClass(view)}
    />
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
          <Check className="size-3 text-success-ink" aria-hidden />
        ) : (
          <Copy className="size-3" aria-hidden />
        )}
      </button>
      {state === "failed" && (
        <span className="shrink-0 text-warning-ink">
          <Trans>复制失败</Trans>
        </span>
      )}
    </div>
  );
}

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
  const failureLabel = failureCodeLabel(projection.failure);

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
      {failureLabel && (
        <p className="mt-2 text-xs text-destructive-ink">{t(failureLabel)}</p>
      )}
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
  /**
   * 反查结果**连同它属于哪个会话**一起存；`target` 为 `undefined` = 还没查完，
   * `null` = 确实没有对应条目。
   *
   * 只存 target 的话，切到另一个会话时旧结果会在新反查回来之前继续渲染——而那条链接指向的是
   * **上一个会话**的收件箱条目，点下去打开的是另一批文件。effect 会重跑，但 state 不会自己
   * 回到「还没查」；节点未就绪时 effect 更是直接早退，旧链接能一直挂着。
   *
   * 渲染期比对而不是在 effect 里先清空：后者对 `phase` 变化也会清一次，链接会闪一下再回来。
   */
  const [resolved, setResolved] = useState<{
    sessionId: string;
    target: { id: string; archived: boolean } | null;
  } | null>(null);
  const target = resolved?.sessionId === sessionId ? resolved.target : undefined;

  useEffect(() => {
    const node = getNode();
    if (!node) return;
    let cancelled = false;
    void (async () => {
      try {
        const detail = await node.inbox_item_by_session(sessionId);
        if (cancelled) return;
        setResolved({
          sessionId,
          target: detail ? { id: detail.id, archived: detail.archivedAt !== null } : null,
        });
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
