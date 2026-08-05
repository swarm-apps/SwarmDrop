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
import { useEffect, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import type { MessageDescriptor } from "@lingui/core";
import {
  calcPercent,
  formatDuration,
  formatFileSize,
  formatTransferRate,
} from "@swarmdrop/shared-view";
import { cn } from "@/lib/cn";
import { PANEL_SURFACE } from "./section";
import { ConfirmAction, INLINE_ACTION_CLASS } from "./confirm-action";
import { OpenListButton } from "./master-detail";
import { ProgressBar } from "./progress-bar";
import { RelativeTime } from "./relative-time";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";
import {
  DIRECTION_LABEL,
  FILE_LIST_LIMIT,
  PHASE_META,
  elapsedSeconds,
  phaseLabel,
  type TransferFileRow,
} from "./transfer-labels";
import {
  isActiveSession,
  isDeletableSession,
  isLostOnReload,
  isPausableSession,
  sessionEndedAt,
  transferSample,
} from "../_lib/format";
import { inboxItemHref } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
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
    // 详情自己是滚动容器（`min-h-0 flex-1` + `overflow-y-auto`）：宽屏下滚详情不会带走
    // 左边的列表，窄屏下滚它也不会把页头顶走。面板级圆角走 18px 词汇，与控件的 8px 分开。
    <div className={cn("flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 sm:p-6", PANEL_SURFACE)}>
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
                      className="size-3 shrink-0 text-success-ink"
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
        <p className="mt-2 text-xs text-red-600 dark:text-red-400">{t(failureLabel)}</p>
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

/**
 * 会话标题：首个文件名 + 「还有几个」的计数徽标。
 *
 * 计数**不并进被截断的那段文字**（「a.zip 等 3 个文件」在窄列里必然被切掉尾巴，而尾巴才是
 * 计数）——它自己占一个 `shrink-0` 的位，永远看得见。
 *
 * `files` 为空只可能出现在异常投影上，那时回落到对端名，至少还认得出是跟谁的会话。
 */
export function SessionTitle({ files, fallback }: { files: TransferProjection["files"]; fallback: string }) {
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
