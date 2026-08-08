"use client";

// 节点状态入口：常驻导航里那枚状态徽章，点开是「这个节点现在怎么样 + 要不要停/起它」。
//
// 三端同一件事的第三份实现：桌面 `src/components/network/node-status-sheet.tsx`、移动
// `mobile/src/components/node-control-sheet.tsx`。形态各随本端（Dialog / BottomSheet），
// **信息分层由 DESIGN.md 的 Node Status Contract 钉死，是两层不是四层**：
//
//   结论层（常驻）  状态点+词 · 可达性**后果句** · 已配对 N·在线 M · 至多一个 CTA
//   诊断层（一个折叠，默认收起）
//                   引导节点逐条（状态 · 归因 · 原样 lastError + 复制）+ 本机真值
//                   （节点 ID / 可达地址 / 身份存放位置 / 运行时长）
//
// 「运行时长」在诊断层而不是结论层：它回答不了上面四个问题里的任何一个。
//
// ## 浏览器这一份少了什么，为什么
//
// 契约的 Degradation 段规定：**NAT 状态与 mDNS 发现数这两格 Web 整格不渲染**——autonat
// 编译期就不在 wasm target 里、浏览器也没有 mDNS，两者是结构性恒定值，摆一个永远
// 「未知 / 0」的字段比缺席更糟。监听地址那格叫「可达地址」而不是「监听地址」，因为它俩
// 说的不是一件事：浏览器不 listen 任何 socket，这里的地址是 reservation 之后才出现的
// circuit 地址。
//
// ## 为什么这里有启停，而设备页注释曾说「不给用户开关」
//
// 那句话（「Web 上由 WebNodeBootstrap 自动接管，给用户一个开关只会让人以为自己需要管它」）
// 说的是**设备页不摆一个常驻开关**，那仍然成立：这里的启停藏在状态徽章之后，用户不会撞见它。
// 但「节点跑不跑」是个可观测的事实，能看见就得能处置——presence 卡住、引导节点全挂时，
// 重启节点是唯一的自救手段，此前只能整页刷新（连带丢掉所有进行中的发送）。

import { Trans, useLingui } from "@lingui/react/macro";
import { ChevronRight, Power, Settings2 } from "lucide-react";
import Link from "next/link";
import { useMemo, useState } from "react";
import type { NodeHealthSummary } from "@swarmdrop/shared-view";
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
import {
  INFRA_LINK_STATE_LABEL,
  INFRA_ROLE_LABEL,
  INFRA_SCOPE_LABEL,
  INFRA_SOURCE_LABEL,
  NODE_HEALTH_MESSAGE,
  TONE_DOT,
} from "../_lib/network-view";
import { stopNodeRuntime } from "../_lib/node-lifecycle";
import { IDENTITY_LOCATION, selectReservation, useWebNode } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { useNodeHealth, useInfraLinkRows, type InfraLinkRow } from "../_lib/use-network-status";
import { useNowSeconds } from "../_lib/use-now-seconds";
import { CopyButton } from "./copy-button";
import { Disclosure } from "./disclosure";
import { NodeStatusPill, useNodeStatusPresentation } from "./node-status-pill";
import { StartNodeButton } from "./start-node-button";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";

export function NodeStatusDialog({
  pillClassName = "",
  labelClassName = "",
  triggerClassName = "",
}: {
  /** 透传给 pill 的形态类（侧栏两档尺寸不同）。 */
  pillClassName?: string;
  labelClassName?: string;
  /**
   * 触发按钮的**外框**类，只该写「盒子多大、圆角多少」这一类。
   *
   * 按钮是个贴合 pill 的透明包装，焦点环画在它身上，所以它的盒子必须与 pill 严丝合缝。
   * 此前它索性不接 className，理由是「两处都能改只会让它们对不齐」——但那条规矩隐含了
   * 「pill 只有一种形状」，而侧栏的图标档现在把它换成了竖排方块。两半仍由**同一个调用方
   * 在同一段 JSX 里**给出（见 `app-nav.tsx` 的 `AppSidebar`），改一半不改另一半在同屏就
   * 看得见，所以对不齐的风险没有回来。
   */
  triggerClassName?: string;
}) {
  const { t } = useLingui();
  const status = useWebNode((s) => s.status);
  // 徽章与它的可访问名读同一份判据——两者各算一遍，`title` 说的和眼睛看到的迟早会不一样。
  const { label: phase } = useNodeStatusPresentation();
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
          //
          // 默认是胶囊（窄屏顶栏与设置页都用这一档）；侧栏图标档把 pill 换成竖排方块，
          // 外框跟着换由 `triggerClassName` 给。
          className={`focus-ring group rounded-full ${triggerClassName}`}
        >
          <NodeStatusPill
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
  const error = useWebNode((s) => s.error);
  const devices = useWebNode((s) => s.pairedDevices);
  const health = useNodeHealth();

  // selector 只返回 store 内的稳定引用，派生一律放这里——`pnpm check:zustand-access` 规则 B。
  const onlineCount = useMemo(
    () => devices.reduce((n, d) => (d.status === "online" ? n + 1 : n), 0),
    [devices],
  );

  /**
   * 诊断层的展开态。
   *
   * 受控是为了 `openDiagnostics` 那个 CTA——「连不上任何网络」时唯一有用的下一步就是
   * 看看每条引导节点各自怎么了，而那份清单就在下面。初值仍是收起：契约要的是**两层**，
   * 默认展开等于把诊断抬进结论层。
   */
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);

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
    // `max-h` + `overflow-y-auto` **不能省**：`DialogContent` 原语两者都不给，而内容是
    // `fixed` + `translate-y-[-50%]`、应用外壳是 `h-dvh overflow-hidden`、Radix 还锁了 body
    // 滚动——高出视口的部分不是被裁掉，是**根本触达不到**。展开「网络诊断」后这个弹窗约
    // 700px（pill 行 + 已配对行 + N 条 infra 行 + 四行本机真值，后者还带 `break-all` 的
    // 节点 ID 与 circuit 地址），手机视口装不下；而它由 `AppMobileHeader`（<768px）唤起，
    // 又是**停止节点的唯一入口**——恰好在移动优先那一档够不着。桌面孪生
    //（`src/components/network/node-status-sheet.tsx`）用的是同一副护栏。
    //
    // 单位是 `dvh` 不是桌面那份的 `vh`：移动端浏览器地址栏展开时 `vh` 算的仍是大视口，
    // 85vh 会越过可见区，等于没设。
    //
    // 内滚给的是**中间那一段**而不是整个 `DialogContent`：后者一滚，`absolute` 的关闭按钮
    // 与底部动作区会跟着内容滚走——而「停止节点」正是本弹窗最该随手够到的东西。
    <DialogContent className="max-h-[85dvh] sm:max-w-md">
      <DialogHeader>
        <DialogTitle>
          <Trans>网络节点</Trans>
        </DialogTitle>
        {/* 结论层信息位 2：可达性的**后果句**。
            不是「良好 / 受限 / 可达」这类无主语形容词——那种说完等于没说，用户仍然不知道
            现在能不能收到别人发来的文件。六个状态各有一句，措辞由 DESIGN.md 的契约表钉死。 */}
        <DialogDescription>{t(NODE_HEALTH_MESSAGE[health.level])}</DialogDescription>
      </DialogHeader>

      {/* `min-h-0` 是这段能缩的前提：`DialogContent` 是 grid，auto 轨道的自动最小尺寸
          默认是内容的最小高度，不写它这段就撑着不缩、`max-h` 形同虚设（滚动容器本身
          自动最小尺寸为 0，两者叠加是双保险）。`overscroll-contain` 挡住滚动链传导。

          `-m-1 p-1` 两者相消，孩子的位置一像素不动，只是让滚动容器多出 4px 内边距——
          `.focus-ring` 是 2px 宽 + 2px 外偏移的 outline，正好 4px，没有这圈余量它会被
          滚动容器齐边裁掉，键盘用户看到的焦点环缺一条边。 */}
      <div className="-m-1 flex min-h-0 flex-col gap-3 overflow-y-auto overscroll-contain p-1">
        {/* 结论层信息位 1（状态点 + 词）与 4（至多一个 CTA）同处一行。 */}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <NodeStatusPill />
          <HealthCta cta={health.cta} onOpenDiagnostics={() => setDiagnosticsOpen(true)} />
        </div>

        {error && <WebErrorCard error={error} className="text-xs" />}

        {running && (
          <>
            {/* 结论层信息位 3：已配对 N · 在线 M，可点进设备页。 */}
            <Link
              href={NAV.devices.href}
              className="focus-ring flex items-center justify-between gap-3 rounded-lg border px-3 py-2 text-xs transition-colors hover:bg-accent/40"
            >
              <span className="text-muted-foreground">
                <Trans>已配对设备</Trans>
              </span>
              <span className="flex items-center gap-1 font-medium tabular-nums text-foreground">
                <Trans>
                  {devices.length} 台 · 在线 {onlineCount}
                </Trans>
                <ChevronRight className="size-3.5 text-muted-foreground" aria-hidden />
              </span>
            </Link>

            {/* ── 诊断层 ─────────────────────────────────────────────────
                一个折叠，默认收起。里面是排查时才要的东西：每条引导节点各自怎么了，
                以及本机的机器真值。 */}
            <Disclosure
              compact
              className="rounded-lg border text-xs"
              label={<Trans>网络诊断</Trans>}
              open={diagnosticsOpen}
              onOpenChange={setDiagnosticsOpen}
            >
              <InfraLinkList />
              <LocalTruth nodeId={nodeId} />
              {/* 看到某条连不上之后总得有地方去改它——这个弹窗只读不写（它管的是节点本身的
                  启停），增删在设置页。少了这一行，诊断层就成了一个只能干看的死胡同。 */}
              <p className="mt-2.5 text-[11px] text-muted-foreground">
                <Trans>
                  引导节点的增删在{" "}
                  <Link href={NAV.settings.href} className="underline underline-offset-2">
                    设置
                  </Link>{" "}
                  的「引导节点」区。
                </Trans>
              </p>
            </Disclosure>
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
 * 结论层的那**一个** CTA。
 *
 * `startNode` 不在这里出——底部动作区已经摆着那颗按钮了，再画一颗就是同一个动作说两遍，
 * 而契约写的是「至多一个」。`null` 同样是合法答案，不为对称造一个。
 */
function HealthCta({
  cta,
  onOpenDiagnostics,
}: {
  cta: NodeHealthSummary["cta"];
  onOpenDiagnostics: () => void;
}) {
  if (cta === "openSettings") {
    return (
      <Button asChild size="sm" variant="outline" className="gap-1.5">
        <Link href={NAV.settings.href}>
          <Settings2 className="size-3.5" aria-hidden />
          <Trans>去设置</Trans>
        </Link>
      </Button>
    );
  }
  if (cta === "openDiagnostics") {
    return (
      <Button size="sm" variant="outline" onClick={onOpenDiagnostics}>
        <Trans>打开诊断</Trans>
      </Button>
    );
  }
  return null;
}

/** 诊断层上半：每条基础设施关系一行。 */
function InfraLinkList() {
  const rows = useInfraLinkRows();
  if (rows.length === 0) {
    return (
      <p className="text-[11px] leading-5 text-muted-foreground">
        <Trans>还没有登记任何引导节点。</Trans>
      </p>
    );
  }
  return (
    <ul className="flex flex-col gap-2">
      {rows.map((row) => (
        <InfraLinkRowView key={row.link.peerId} row={row} />
      ))}
    </ul>
  );
}

function InfraLinkRowView({ row }: { row: InfraLinkRow }) {
  const { t } = useLingui();
  const { link, presentation } = row;
  // 归因：来源 · 范围 · 角色。同一条关系可以既是 DHT 种子又是中继（两个角色正交），
  // 所以角色是**列举**不是二选一。
  const roles = [
    link.roles.kadServer ? t(INFRA_ROLE_LABEL.kadServer) : null,
    link.roles.relayServer ? t(INFRA_ROLE_LABEL.relayServer) : null,
  ].filter((r): r is string => r !== null);
  const attribution = [
    ...link.sources.map((source) => t(INFRA_SOURCE_LABEL[source])),
    t(INFRA_SCOPE_LABEL[link.scope]),
    ...roles,
  ].join(" · ");

  return (
    <li className="flex flex-col gap-1 rounded-md border bg-background/50 px-2 py-1.5">
      <div className="flex items-center gap-1.5">
        <StatusDot
          colorClass={TONE_DOT[presentation.tone]}
          pulse={presentation.state === "settling"}
        />
        <span className="shrink-0 font-medium text-foreground">
          {t(INFRA_LINK_STATE_LABEL[presentation.state])}
        </span>
        <span className="min-w-0 flex-1 truncate text-right font-mono text-[11px] text-muted-foreground">
          …{link.peerId.slice(-8)}
        </span>
      </div>
      <span className="text-[11px] text-muted-foreground">{attribution}</span>
      {/* `lastError` 原样、不翻译——排查时用户要贴进 issue、跟日志比对的就是这一句。
          配复制按钮：长机器文本不可选中却长得像可点，正好违反复制可供性那条规矩。 */}
      {presentation.detail && (
        <div className="flex items-start gap-1">
          <p className="min-w-0 flex-1 break-words text-[11px] text-destructive-ink">
            {presentation.detail}
          </p>
          <CopyButton
            key={presentation.detail}
            value={presentation.detail}
            label={t`复制错误信息`}
            className="h-6 px-1.5"
          />
        </div>
      )}
    </li>
  );
}

/** 诊断层下半：本机真值。 */
function LocalTruth({ nodeId }: { nodeId: string | null }) {
  const { t } = useLingui();
  const reservation = useWebNode(selectReservation);
  const startedAt = useWebNode((s) => s.startedAt);

  return (
    <dl className="mt-2.5 flex flex-col gap-2.5">
      <DiagnosticRow label={<Trans>节点 ID</Trans>}>
        <span className="min-w-0 flex-1 font-mono tabular-nums break-all">{nodeId ?? "—"}</span>
        {nodeId && <CopyButton key={nodeId} value={nodeId} label={t`复制节点 ID`} />}
      </DiagnosticRow>
      {/* 标题是「可达地址」不是「监听地址」：浏览器不 listen 任何 socket，这里的地址是
          reservation 建立之后才出现的 circuit 地址，两者说的不是一件事（契约的
          Permitted divergence 段写死了这条分叉）。 */}
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
      <DiagnosticRow label={<Trans>身份存放位置</Trans>}>
        <span className="flex-1 font-mono">{IDENTITY_LOCATION}</span>
      </DiagnosticRow>
      <DiagnosticRow label={<Trans>运行时长</Trans>}>
        <span className="flex-1 tabular-nums">
          <Uptime startedAt={startedAt} />
        </span>
      </DiagnosticRow>
    </dl>
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

function DiagnosticRow({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="flex items-start gap-2 text-foreground">{children}</dd>
    </div>
  );
}

/**
 * 已运行多久。**在诊断层**——它回答不了「别人能不能连到我」，进不了结论层。
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
