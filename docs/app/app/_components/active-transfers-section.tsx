"use client";

// 设备页的「活跃传输」区块 —— 传输页的入口，也是它离开常驻导航后的**可见性补偿**。
//
// ## 它不是首页摘要，是一条被拿掉的导航项的替身
//
// 侧栏收敛成三项之后，「有东西正在传」这件事失去了它在 chrome 上的位置。知识库那条
// 「拆多路由会藏起有时效的东西，导航徽标是补偿不是装饰」讲的就是这类退化——区别只在于
// 这次补偿的形态从一个数字变成了几行真内容：既然设备页本来就是首页，直接把进行中的会话
// 摆出来，比在导航上挂一个「3」更有用。
//
// 移动端设备页是同一个形态（`mobile/src/app/(main)/index.tsx` 的「活跃传输 · 查看全部」），
// 两端的信息与出口一一对应。
//
// ## 只列进行中，且列不下要说出来
//
// 历史归传输页。这里最多三行——超过就折成一句「还有 N 条」而不是静默截断
// （静默截断读起来像「就这些了」）。

import { Trans, useLingui } from "@lingui/react/macro";
import { ArrowDownToLine, ArrowLeftRight, ArrowUpFromLine } from "lucide-react";
import Link from "next/link";
import { useMemo } from "react";
import { EtaText } from "./eta-text";
import { INLINE_ACTION_CLASS } from "./confirm-action";
import { ROW_SURFACE, SectionHeader, SectionShell } from "./section";
import { ProgressBar } from "./progress-bar";
import { StatusDot } from "./status-dot";
import { SessionTitle } from "./session-title";
import { PHASE_META, phaseLabel } from "./transfer-labels";
import { isActiveSession, sortByUpdatedDesc, transferSample } from "../_lib/format";
import { NAV, transferSessionHref } from "../_lib/nav";
import { useWebNode } from "../_lib/store";
import { useSessionPublishing } from "../_lib/use-session-publishing";
import { useUsableRates } from "../_lib/use-usable-rates";
import type { TransferProjection } from "../_lib/view-types";

/** 首页最多摆几行。再多就该去传输页——那里有筛选，这里没有。 */
const PREVIEW_LIMIT = 3;

export function ActiveTransfersSection() {
  const { t } = useLingui();
  const projections = useWebNode((s) => s.projections);
  const ready = useWebNode((s) => s.status === "running");

  // selector 只返回 store 内的稳定引用，派生放这里——`pnpm check:zustand-access` 规则 B。
  const active = useMemo(
    () => sortByUpdatedDesc(Object.values(projections)).filter(isActiveSession),
    [projections],
  );
  const shown = active.slice(0, PREVIEW_LIMIT);
  const hidden = active.length - shown.length;

  // 空的时候整块塌成一行。
  //
  // 此前空态是「面板标题 + 一行居中灰字」，撑出约 175px 的空腔——而这块区块**大部分时间
  // 都是空的**（传输是偶发事件，不是常驻内容）。于是设备页的稳定形态就是三个等大的空盒子。
  //
  // 但不能整块不渲染：它是传输页离开常驻导航后**唯一**的入口，藏起来等于让 `/app/transfer`
  // 从应用里消失（DESIGN.md：从 chrome 里拿掉的东西必须在用户已经在的地方重新出现）。
  // 所以留一行——出口还在，空腔没了。
  if (shown.length === 0) {
    return (
      <SectionShell className="flex-row items-center justify-between gap-[var(--space-in-group)] py-3">
        <p className="flex min-w-0 items-center gap-2.5 text-sm text-muted-foreground">
          <span className="glass-control flex size-7 shrink-0 items-center justify-center rounded-full text-brand">
            <ArrowLeftRight className="size-3.5" aria-hidden />
          </span>
          {/* 「没有」得是真的没有：节点没起来时历史还没回补，这时说「现在没有正在进行的
              传输」是在断言一件当时并不成立的事（同 device-grid 与 send-panel 那两处）。 */}
          <span className="truncate">
            {ready ? (
              <Trans>现在没有正在进行的传输</Trans>
            ) : (
              <Trans>节点起来后，进行中的传输会出现在这里</Trans>
            )}
          </span>
        </p>
        {/* `aria-label` 而不是光靠可见文案：塌成一行后这块不再渲染 `SectionHeader`，
            section 没有可访问名，读屏的链接列表里这条「查看全部」与别处的无法区分。 */}
        <Link
          href={NAV.transfer.href}
          aria-label={t`查看全部传输`}
          className={`${INLINE_ACTION_CLASS} shrink-0 text-xs`}
        >
          {t`查看全部`}
        </Link>
      </SectionShell>
    );
  }

  return (
    <SectionShell>
      <SectionHeader
        icon={ArrowLeftRight}
        title={<Trans>活跃传输</Trans>}
        count={active.length}
        action={
          <Link href={NAV.transfer.href} className={`${INLINE_ACTION_CLASS} text-xs`}>
            {t`查看全部`}
          </Link>
        }
      />

      <ul className="flex flex-col gap-[var(--space-in-group)]">
        {shown.map((projection) => (
          <li key={projection.sessionId}>
            <ActiveTransferRow projection={projection} />
          </li>
        ))}
      </ul>

      {hidden > 0 && (
        <p className="text-center text-[11px] text-muted-foreground">
          <Trans>
            还有 {hidden} 条进行中，
            <Link href={NAV.transfer.href} className="underline underline-offset-2">
              去传输页
            </Link>
            查看。
          </Trans>
        </p>
      )}
    </SectionShell>
  );
}

function ActiveTransferRow({ projection }: { projection: TransferProjection }) {
  const { t } = useLingui();
  // 实时采样：projection 只在状态转换时重发，进度条要跟手就得读这一路。
  //
  // **必须经 `transferSample` 取 `live`，不能直读 `s.progress[id]`**：Web 的 progress 域
  // 只增不清（删除路径一概不动它），直读会让已结束的会话顶着最后一帧陈旧采样，显示一句
  // 「剩余 3m 20s」。同一个洞在 2026-07-28 表现为「已完成 · 23%」。
  const progress = useWebNode((s) => s.progress[projection.sessionId]);
  const { live, percent } = transferSample(projection, progress);
  // 剩余时间**过一遍时效**：那一帧躺得太久就退回占位，而不是继续报一个早已不成立的数
  // （判据与「陈旧那一刻怎么被重算」都在 `useUsableRates` 里）。
  const { eta } = useUsableRates(projection.sessionId, live);
  const active = projection.phase === "active";
  // 此刻正在保存哪个文件（接收的「暂存 → 发布」第二段）；没有则 null。
  // 「只有 active 会话才谈得上发布」这条收窄折在 hook 里（见那里的说明）。
  const publishing = useSessionPublishing(projection);
  const receiving = projection.direction === "receive";
  const DirectionIcon = receiving ? ArrowDownToLine : ArrowUpFromLine;

  return (
    <Link
      href={transferSessionHref(projection.sessionId)}
      className={`${ROW_SURFACE} flex min-h-11 flex-col gap-1.5 px-3 py-2.5 transition-colors hover:bg-accent`}
    >
      <div className="flex min-w-0 items-center gap-2">
        <DirectionIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <SessionTitle files={projection.files} fallback={t`未命名传输`} />
        <span className="shrink-0 font-mono text-[11px] tabular-nums text-muted-foreground">
          {percent}%
        </span>
      </div>
      {/*
        `label={null}`：整行是个链接，而 ARIA 对 link 同样规定 Children Presentational——
        写在这里的 `aria-label` 会被读屏整个丢弃。进度信息由链接自己的可访问名承担
        （文件名与百分比本就是它的后代文本），见 progress-bar.tsx。
      */}
      <ProgressBar percent={percent} tone={publishing ? "local" : "transfer"} label={null} />
      {/* 阶段行的右端此前是空的。设备页就是 Web 应用的首页，这一行是**全端成本最低、
          可见性最高**的一个信息位——「还要多久」正是用户看一眼就想知道的那件事，
          而速率（要用户自己拿剩余字节去除）留给详情侧。 */}
      <p className="flex items-center justify-between gap-1.5 text-[11px] text-muted-foreground">
        <span className="flex min-w-0 items-center gap-1.5">
          <StatusDot colorClass={PHASE_META[projection.phase].dot} />
          {/* 发布期把阶段词换掉：那一刻字节已经收齐，仍说「传输中」就是在描述一件已经
              结束了的事，而真正还在跑的是本机的落盘。 */}
          <span className="truncate">
            {publishing ? t`正在保存…` : t(phaseLabel(projection))}
          </span>
        </span>
        {/* 只在 active 出现：暂停 / 等待接受的会话没有速率，摆一个剩余时间是在报告一段
            并不存在的等待。

            发布期同理**给占位而不是给数**：那一刻字节早已收齐、网上什么都没在跑，最后那帧
            的「剩余 5s」描述的是一段已经结束了的等待——与这一行左端刚说完的「正在保存…」
            正面打架。格子不能塌（契约：ETA 永不丢），所以传 null 让它退回「计算中」。 */}
        {active && (
          <span className="shrink-0 tabular-nums">
            <EtaText seconds={publishing ? null : eta} />
          </span>
        )}
      </p>
    </Link>
  );
}
