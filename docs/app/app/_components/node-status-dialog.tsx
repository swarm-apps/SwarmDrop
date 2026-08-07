"use client";

// 节点状态入口：常驻导航里那枚状态徽章，点开是「这个节点现在怎么样 + 要不要停/起它」。
//
// 三端同一件事的第三份实现：桌面 `src/components/network/stop-node-sheet.tsx`、移动
// `mobile/src/components/node-control-sheet.tsx`。形态各随本端（Dialog / BottomSheet），
// **信息分层是同一套**：
//
//   始终可见   状态 + 已运行多久 + 有几台设备连得上
//   渐进披露   节点 ID、可达地址、中继、身份存放位置（libp2p 细节留给需要的人）
//   动作       running → 停止；其余 → 启动
//
// ## 浏览器这一份少了什么，为什么
//
// 桌面/移动的诊断里有监听地址、NAT 状态、引导节点连通性。浏览器**不 listen 任何 socket**，
// 这三项在这里没有对应物——它的可达性完全由 relay circuit 承担，所以那一栏换成中继与
// circuit 地址。不是省略，是这一端的等价物。
//
// ## 为什么这里有启停，而设备页注释曾说「不给用户开关」
//
// 那句话（「Web 上由 WebNodeBootstrap 自动接管，给用户一个开关只会让人以为自己需要管它」）
// 说的是**设备页不摆一个常驻开关**，那仍然成立：这里的启停藏在状态徽章之后，用户不会撞见它。
// 但「节点跑不跑」是个可观测的事实，能看见就得能处置——presence 卡住、relay 全挂时，
// 重启节点是唯一的自救手段，此前只能整页刷新（连带丢掉所有进行中的发送）。

import { Trans, useLingui } from "@lingui/react/macro";
import { Power } from "lucide-react";
import Link from "next/link";
import { useMemo } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { NAV } from "../_lib/nav";
import { stopNodeRuntime } from "../_lib/node-lifecycle";
import { selectReservation, useWebNode } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { useNowSeconds } from "../_lib/use-now-seconds";
import { CopyButton } from "./copy-button";
import { NodeStatusPill, useNodeStatusLabel } from "./node-status-pill";
import { StartNodeButton } from "./start-node-button";
import { WebErrorCard } from "./web-error-view";

export function NodeStatusDialog({
  pillClassName = "",
  labelClassName = "",
}: {
  /** 透传给 pill 的形态类（侧栏两档尺寸不同）。触发按钮自己不接 className——它只是个
   *  贴合 pill 的透明包装，形状要跟着 pill 走，两处都能改只会让它们对不齐。 */
  pillClassName?: string;
  labelClassName?: string;
}) {
  const { t } = useLingui();
  const status = useWebNode((s) => s.status);
  const phase = useNodeStatusLabel(status);
  // 「节点」+ 状态词拼在一起是给读屏与 tooltip 的完整句；可见文本只留状态词（空间有限）。
  const label = t`节点${phase}`;

  return (
    <Dialog>
      <DialogTrigger asChild>
        <button
          type="button"
          // 状态本身是 status role 的内容，但它同时是个按钮——两者不能挂在同一个节点上
          // （button 的后代角色会被辅助技术丢弃，见 progress-bar.tsx 那条同源的坑）。
          // 所以可访问名由这里给全，pill 只负责画。
          title={t`${label}，点开查看详情`}
          aria-label={t`${label}，点开查看详情`}
          data-testid="node-status-trigger"
          data-node-status={status}
          // hover 反馈走**底色**（`group-hover:bg-accent`，画在 pill 上）而不是此前的
          // `hover:opacity-80`：侧栏底部同一块里还有三枚 ghost 按钮，它们的 hover 就是底色，
          // 两种语言并存会让唯一那个重要入口反而看着最不像能点的。透明度在半透明的
          // `glass-rail` 上本来也几乎看不出变化。
          className="focus-ring group rounded-full"
        >
          <NodeStatusPill
            status={status}
            labelClassName={labelClassName}
            className={`transition-colors group-hover:border-foreground/20 group-hover:bg-accent ${pillClassName}`}
          />
        </button>
      </DialogTrigger>
      <NodeStatusDialogContent />
    </Dialog>
  );
}

function NodeStatusDialogContent() {
  const { t } = useLingui();
  const status = useWebNode((s) => s.status);
  const nodeId = useWebNode((s) => s.nodeId);
  const startedAt = useWebNode((s) => s.startedAt);
  const error = useWebNode((s) => s.error);
  const devices = useWebNode((s) => s.pairedDevices);
  const relays = useWebNode((s) => s.relays);
  const reservation = useWebNode(selectReservation);

  // selector 只返回 store 内的稳定引用，派生一律放这里——`pnpm check:zustand-access` 规则 B。
  const onlineCount = useMemo(
    () => devices.reduce((n, d) => (d.status === "online" ? n + 1 : n), 0),
    [devices],
  );
  const activeRelayCount = useMemo(
    () => relays.reduce((n, r) => (r.state === "active" ? n + 1 : n), 0),
    [relays],
  );

  const running = status === "running";
  /**
   * 动作区显示哪一颗按钮。
   *
   * **判据是「有没有一个节点可停」而不是 `running`**：关停在途（`closing`）时 `running`
   * 已经是 false，照它分支就会在停的过程中换成一颗灰着的「启动节点」——看起来像点错了地方，
   * 而真正在发生的事（正在停）反而没人说。
   *
   * 当前实测停不到一帧（8ms 采样一次都没采到 `closing`），所以这个中间态**现在看不见**。
   * 保留它是因为它便宜、且成立的条件不在这一侧：一旦关停要断的连接多起来、或要先取消
   * 进行中的传输，`closing` 就会变成一段真实可见的时间。
   */
  const stoppable = running || status === "closing";

  return (
    <DialogContent className="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>
          <Trans>网络节点</Trans>
        </DialogTitle>
        <DialogDescription>
          {running ? (
            <Trans>节点在跑时，已配对的设备才能找到你并发起传输。</Trans>
          ) : (
            <Trans>节点停着时收不到任何请求，已配对的设备会把你看作离线。</Trans>
          )}
        </DialogDescription>
      </DialogHeader>

      <div className="flex flex-col gap-3">
        <NodeStatusPill status={status} className="self-start" />

        {error && <WebErrorCard error={error} className="text-xs" />}

        {running && (
          <>
            <dl className="divide-y overflow-hidden rounded-lg border text-xs">
              <SummaryRow label={<Trans>已配对设备</Trans>}>
                <Trans>
                  {devices.length} 台 · 在线 {onlineCount}
                </Trans>
              </SummaryRow>
              <SummaryRow label={<Trans>中继</Trans>}>
                {activeRelayCount > 0 ? (
                  <Trans>{activeRelayCount} 条可达</Trans>
                ) : (
                  <Trans>连接中…</Trans>
                )}
              </SummaryRow>
              <SummaryRow label={<Trans>运行时长</Trans>}>
                <Uptime startedAt={startedAt} />
              </SummaryRow>
            </dl>

            {/*
              渐进披露：排查时才要的机器真值，不占默认视线。
              **只放运行时事实**——「身份存放在哪里」看着也像诊断，但它回答的是「我刷新后
              还是我吗」，属于持久身份，归设置页的「本机节点」区（那里同时管改名）。
              节点 ID 两处都有则是刻意的：它是排查时最常要复制的一串，三端的节点弹窗都带。
            */}
            <details className="rounded-lg border px-3 py-2 text-xs">
              <summary className="focus-ring -mx-1 cursor-pointer rounded px-1 text-muted-foreground">
                <Trans>网络诊断</Trans>
              </summary>
              <dl className="mt-2 flex flex-col gap-2.5">
                <DiagnosticRow label={<Trans>节点 ID</Trans>}>
                  <span className="min-w-0 flex-1 font-mono tabular-nums break-all">
                    {nodeId ?? "—"}
                  </span>
                  {nodeId && <CopyButton key={nodeId} value={nodeId} label={t`复制节点 ID`} />}
                </DiagnosticRow>
                <DiagnosticRow label={<Trans>可达地址</Trans>}>
                  {reservation ? (
                    <>
                      <span className="min-w-0 flex-1 font-mono break-all">{reservation}</span>
                      <CopyButton key={reservation} value={reservation} label={t`复制可达地址`} />
                    </>
                  ) : (
                    <span className="flex-1 text-muted-foreground">
                      {/* 浏览器不 listen，没有 circuit 就等于对外不存在——这句得说清楚，
                          否则用户会以为「生成邀请」是坏了。 */}
                      <Trans>还没有可达地址，对方暂时拨不进来。</Trans>
                    </span>
                  )}
                </DiagnosticRow>
              </dl>
              <p className="mt-2.5 text-[11px] text-muted-foreground">
                <Trans>
                  引导节点的增删与连通性测试在{" "}
                  <Link href={NAV.settings.href} className="underline underline-offset-2">
                    设置
                  </Link>{" "}
                  的「引导节点」区。
                </Trans>
              </p>
            </details>
          </>
        )}
      </div>

      <DialogFooter>
        {stoppable ? <StopButton /> : <StartNodeButton testId="node-start-button" className="w-full" />}
      </DialogFooter>
    </DialogContent>
  );
}

/**
 * 停止：destructive，且必须说清后果。
 *
 * **不套第二层确认对话框**——点开这个弹窗本身已经是一次显式导航，按钮旁边那句话把代价
 * 说全了。设备卡片上的「取消配对」需要确认是因为它就摆在列表里，一次误点即成事实。
 */
function StopButton() {
  // 同 `StartNodeButton`：在途状态的真源是 store 而不是本次点击——节点也可能是别处停的。
  const closing = useWebNode((s) => s.status === "closing");
  const action = useAsyncAction();
  const pending = closing || action.pending;
  return (
    <div className="flex w-full flex-col gap-2">
      <p className="text-[11px] text-muted-foreground">
        <Trans>停止后会断开所有连接，进行中的传输将中断。</Trans>
      </p>
      <Button
        variant="destructive"
        disabled={pending}
        onClick={() => action.run(stopNodeRuntime)}
        data-testid="node-stop-button"
        className="gap-1.5"
      >
        <Power className="size-4" aria-hidden />
        {pending ? <Trans>停止中…</Trans> : <Trans>停止节点</Trans>}
      </Button>
      {/* 失败就地显示：弹窗留在原位，重试只差一次点击（同 unpair-dialog 的判据）。 */}
      {action.error && <WebErrorCard error={action.error} className="text-xs" />}
    </div>
  );
}

function SummaryRow({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3 px-3 py-2">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="text-right font-medium tabular-nums text-foreground">{children}</dd>
    </div>
  );
}

function DiagnosticRow({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="flex items-start gap-2 text-foreground">{children}</dd>
    </div>
  );
}

/**
 * 已运行多久。
 *
 * 走共享节拍（`useNowSeconds`，30 秒一跳）而不是自建 `setInterval`：同屏的相对时间读同一个
 * 「现在」，也不会为一个弹窗多留一个定时器。分钟以下不报数字——「刚刚启动」比「0 分钟」诚实。
 *
 * **不复用桌面的 `formatUptime`**：那份返回硬编码中文字符串，而这里要的是可翻译节点。
 */
function Uptime({ startedAt }: { startedAt: number | null }) {
  const now = useNowSeconds();
  if (!startedAt) return <>—</>;

  const minutes = Math.floor(Math.max(0, now - Math.floor(startedAt / 1000)) / 60);
  if (minutes < 1) return <Trans>刚刚启动</Trans>;
  if (minutes < 60) return <Trans>{minutes} 分钟</Trans>;

  const hours = Math.floor(minutes / 60);
  return (
    <Trans>
      {hours} 小时 {minutes % 60} 分钟
    </Trans>
  );
}

