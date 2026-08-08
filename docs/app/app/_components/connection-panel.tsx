"use client";

// 引导节点：浏览器怎么被别人找到。
//
// **组件名仍是 `ConnectionPanel`、store 里的域仍是 `relays`**——那是内核事实（这里登记的
// 确实是 relay），改名只发生在**面向用户的那一层**，理由写在下面 `SettingsSection` 旁边。
// 别为了对齐标题去改 wasm 导出或 store 字段名。
//
// ## 浏览器不监听端口，所以「可达」要自己建
//
// 桌面/移动会 listen 本地 socket，对端直接拨得到。浏览器不行，必须先经一个引导节点建立
// circuit 可达地址，那个地址才是邀请里写给对方的东西。这块的全部内容都在服务这一件事。
//
// ## 清单来自内核，不是本地记的（2026-08-04 改）
//
// 此前这里只有一个文本框 + 三个按钮，且「撤销」用的是 `helperIdRef`——**只记手动登记的
// 那一次**。而 `web-node-bootstrap` 启动时就把配置里的全部 helper 自动 ensure 了一遍，
// 那些条目用户既看不到、也撤不掉：填错地址后后台会一直退避重试，界面上一片安静。
//
// 清单来自 store 的 `relays` 域，由 layout 的 `startRelayWatch` 单点订阅 `relays_changed()`
// 写入（两条 wasm 早就导出、前端一直零调用）。**订阅不在这个面板里**：设备页的配对区也要读
// 同一份事实（判断能不能生成邀请），绑在某一页上会让直接进设备页的用户看到一个永远禁用的
// 「生成邀请」——见 `_lib/relay-watch.ts` 的头注释。于是清单里手动与自动登记的一视同仁，
// 每条都能看到状态、失败原因，也都能撤。
//
// 连接类型（局域网/打洞/中继）是真实状态，要像安全工具一样清楚可视化（PRODUCT.md 原则 2），
// 配色沿用桌面 device-card 的语义编码（DESIGN.md：这三色是状态语义编码，在 one-accent 规则之外）。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import { Plus, RadioTower, ShieldCheck, Trash2 } from "lucide-react";
import Link from "next/link";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/cn";
import { getNode } from "../_lib/node-runtime";
import { NAV } from "../_lib/nav";
import {
  WEB_RELAY_HELPERS,
  WEB_RELAY_PEER_IDS,
  bootstrapPeerId,
  bootstrapTransport,
  truncateAddr,
} from "../_lib/relay-helpers";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { selectReservation, useWebNode, webNodeActions } from "../_lib/store";
import {
  toWebError,
  type PathKindJson,
  type RelayInfoJson,
  type RelayStateKind,
} from "../_lib/view-types";
import { SettingsCard, SettingsRow, SettingsSection } from "./settings-primitives";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";

const PATH_META: Record<PathKindJson, { label: MessageDescriptor; dot: string }> = {
  local: { label: msg`局域网直连`, dot: "bg-success" },
  direct: { label: msg`打洞直连`, dot: "bg-info" },
  relayed: { label: msg`中继`, dot: "bg-warning" },
};

const RELAY_META: Record<RelayStateKind, { label: MessageDescriptor; dot: string }> = {
  connecting: { label: msg`连接中`, dot: "bg-warning" },
  active: { label: msg`可达`, dot: "bg-success" },
  failed: { label: msg`连接失败`, dot: "bg-destructive" },
};

export function ConnectionPanel() {
  const { t } = useLingui();
  const nodeStatus = useWebNode((s) => s.status);
  const connection = useWebNode((s) => s.connection);
  // relay 清单由 layout 的 `startRelayWatch` 单点订阅并写进 store——**订阅不在这个面板里**，
  // 因为设备页的配对区也要读它（见 relay-watch.ts 的头注释）。这里只是它的一个视图。
  const relays = useWebNode((s) => s.relays);
  const activeCircuit = useWebNode(selectReservation);

  const [addr, setAddr] = useState("");
  /** 添加入口默认收起：这是极低频动作，常驻的输入框 + 两颗按钮占了整张卡三分之一。 */
  const [showInput, setShowInput] = useState(false);
  const connectAction = useAsyncAction();
  const addAction = useAsyncAction();
  const dropAction = useKeyedAsyncAction();

  const ready = nodeStatus === "running";

  /**
   * 把内核下发的运行态切成「内置 / 自定义」两半。
   *
   * 判据是 **peer id**：`RelayInfoJson` 只有 `id`，不带地址（见 `relay-helpers.ts` 的
   * `bootstrapPeerId`）。内置项的 id 从 `WEB_RELAY_HELPERS` 的 multiaddr 尾部解出来，
   * 两边比对。
   */
  const customRelays = useMemo(
    () => relays.filter((r) => !WEB_RELAY_PEER_IDS.has(r.id)),
    [relays],
  );
  /** 某条内置地址对应的运行态；`undefined` = 它此刻不在清单里（被撤过或还没连上）。 */
  const builtinRelayOf = (helperAddr: string) => {
    const peerId = bootstrapPeerId(helperAddr);
    return peerId === null ? undefined : relays.find((r) => r.id === peerId);
  };

  const closeInput = () => {
    setShowInput(false);
    setAddr("");
  };


  const doConnect = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    connectAction.run(
      () => node.connect(addr.trim()),
      (conn) => webNodeActions.setConnection(conn),
    );
  };

  const doAdd = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    addAction.run(
      async () => {
        const helperId = node.relays_ensure(addr.trim());
        // 不 await `until_active`：登记是**常驻意图**，后台会持续收敛。挂在这里等 30 秒
        // 只会让按钮转半天，而清单里那一条已经在显示「连接中」了。
        return helperId;
      },
      () => {
        closeInput();
        toast.success(t`已添加引导节点，正在连接…`);
      },
    );
  };

  /**
   * 把撤掉的内置节点重新登记回去。
   *
   * 内置项能撤（理由见界面上那段注释），但撤了之后没有回头路就成了单向门——
   * 用户得自己从文档里把那串 multiaddr 找回来手打。`relays_ensure` 是幂等的，
   * 直接用清单里的原串重登记即可。
   *
   * **自带一份 keyed action，不复用 `addAction`**：那个的错误卡只渲染在折叠展开的输入区里
   * （`showInput` 为真时），而这颗按钮长在「默认入口」栅格上，那时输入区是收起的——
   * 失败会零反馈，用户只会一直点。失败走 toast，它的生命周期与折叠态无关；
   * pending 也按 helperAddr 键，连点不会叠发多次 ensure。
   */
  const restoreAction = useKeyedAsyncAction();
  const doRestore = (helperAddr: string) => {
    const node = getNode();
    if (!node) return;
    void restoreAction.run(helperAddr, async () => {
      try {
        node.relays_ensure(helperAddr);
        toast.success(t`已恢复引导节点，正在连接…`);
      } catch (e) {
        toast.error(t`恢复引导节点失败`, { description: toWebError(e).message });
        throw e;
      }
    });
  };

  const doDrop = (helperId: string) => {
    const node = getNode();
    if (!node) return;
    void dropAction.run(helperId, async () => {
      await node.relays_drop(helperId);
      toast.success(t`已移除引导节点`);
    });
  };

  return (
    // 标题、图标与内部行标题都取自桌面 `settings/-bootstrap-nodes-section.tsx`
    //（「引导节点」+ `RadioTower`，行是「默认入口 / 自定义节点」）。
    //
    // **两端管的东西在技术上不完全相同，但对用户是同一件事**：桌面那块配的是 DHT 首次发现用的
    // bootstrap peer，这里配的是建立 circuit 可达地址用的 relay。而本仓自建的那台
    // `47.115.172.218` 同时扮演这两个角色（见 CLAUDE.md 的 Bootstrap / relay node），
    // 用户看到的都是「让我接入网络的那个公网节点」。叫两个名字只会让人以为要配两次。
    <SettingsSection
      icon={RadioTower}
      title={<Trans>引导节点</Trans>}
      aside={
        <Badge variant="outline" className="rounded-full text-[10px]">
          {customRelays.length > 0 ? (
            <Trans>自定义 {customRelays.length}</Trans>
          ) : (
            <Trans>默认</Trans>
          )}
        </Badge>
      }
    >
      <SettingsCard>
        {/* ── 默认入口 ─────────────────────────────────────────────────────
            内置清单（`WEB_RELAY_HELPERS`）此前只当输入框的 placeholder 用，界面上
            根本看不到「默认连的是哪几台」——用户唯一能看到的是内核下发的运行态，
            而那份数据不带地址，只有一截 peer id。桌面把这块摆成一个只读栅格，这里照做。

            **但不加「只读」徽标**：桌面的默认节点确实删不掉，这里的能删——启动时会自动
            ensure 全部内置项，填错的那条会在后台无限退避重试而界面一片安静，所以每条都
            必须撤得掉（那是已修过的缺陷，别退回去）。徽标只写「默认」，不谎称只读。 */}
        <div className="border-b p-4">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <span className="text-sm font-medium text-foreground">
                <Trans>默认入口</Trans>
              </span>
              <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
                <Trans>
                  浏览器不能被直接拨号，需要经引导节点建立一个可达地址，对方才找得到你。内置节点会自动连接，通常无需调整。
                </Trans>
              </span>
            </div>
          </div>

          <div className="mt-3 grid gap-2 sm:grid-cols-2">
            {WEB_RELAY_HELPERS.map((helperAddr) => {
              const relay = builtinRelayOf(helperAddr);
              return (
                <BootstrapNodeCard
                  key={helperAddr}
                  addr={helperAddr}
                  relay={relay}
                  busy={relay !== undefined && dropAction.isPending(relay.id)}
                  disabled={!ready}
                  onDrop={doDrop}
                  onRestore={() => doRestore(helperAddr)}
                />
              );
            })}
          </div>
        </div>

        {/* ── 自定义节点 ──────────────────────────────────────────────────── */}
        <div className="border-b p-4">
          <span className="text-sm font-medium text-foreground">
            <Trans>自定义节点</Trans>
          </span>
          <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
            <Trans>仅在需要接入私有或备用网络时添加。</Trans>
          </span>

          {customRelays.length > 0 ? (
            <ul className="mt-3 grid gap-2">
              {customRelays.map((relay) => (
                <li
                  key={relay.id}
                  className="flex min-w-0 items-center justify-between gap-3 rounded-xl border bg-background/55 p-3 dark:bg-white/[0.035]"
                >
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-medium text-foreground">
                      <RelayStateLabel relay={relay} />
                    </p>
                    {/* 引导节点身份是机器真值 → mono（Mono Truth Rule）。内核只下发 id、
                        不下发地址，所以自定义那条这里只能显示 id。 */}
                    <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                      …{relay.id.slice(-8)}
                    </p>
                    <RelayError message={relay.lastError} />
                  </div>
                  <DropRelayButton
                    onDrop={() => doDrop(relay.id)}
                    disabled={!ready || dropAction.isPending(relay.id)}
                  />
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-3 rounded-xl border border-dashed bg-background/35 px-3 py-4 text-xs leading-5 text-muted-foreground dark:bg-white/[0.025]">
              <Trans>当前使用内置引导节点。</Trans>
            </p>
          )}

          {dropAction.latestError && (
            <WebErrorCard error={dropAction.latestError} className="mt-2 text-xs" />
          )}
        </div>

        {/* ── 添加入口：默认折叠成一行 ──────────────────────────────────────
            此前输入框 + 两颗按钮常驻，占了这张卡三分之一，而添加自定义节点是**极低频**
            动作（多数用户一辈子不做一次）。同桌面：收成一行，点开才出输入框。 */}
        {showInput ? (
          <div className="flex flex-col gap-2 border-b p-4 last:border-b-0">
            <Input
              className="h-11 font-mono text-xs sm:h-9"
              placeholder={WEB_RELAY_HELPERS[0] ?? "/ip4/…/p2p/12D3Koo…"}
              aria-label={t`引导节点地址`}
              value={addr}
              autoFocus
              onChange={(e) => setAddr(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  doAdd();
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  closeInput();
                }
              }}
              disabled={!ready}
            />
            <p className="text-xs leading-5 text-muted-foreground">
              <Trans>
                自建节点的地址形如{" "}
                <code className="font-mono text-[11px]">/ip4/…/udp/…/webrtc-direct/certhash/…/p2p/12D3Koo…</code>
              </Trans>
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                onClick={doAdd}
                disabled={!ready || !addr.trim() || addAction.pending}
              >
                {addAction.pending ? <Trans>添加中…</Trans> : <Trans>添加</Trans>}
              </Button>
              {/* 「只连一次看看通不通」是排查动作，与「登记为常驻引导节点」不是一回事——
                  此前两者是并排的两个按钮，标签还一个中文一个裸英文 "connect"。 */}
              <Button
                size="sm"
                variant="outline"
                onClick={doConnect}
                disabled={!ready || !addr.trim() || connectAction.pending}
              >
                {connectAction.pending ? <Trans>测试中…</Trans> : <Trans>测试连通性</Trans>}
              </Button>
              <Button size="sm" variant="ghost" onClick={closeInput}>
                <Trans>取消</Trans>
              </Button>
            </div>
            {addAction.error && <WebErrorCard error={addAction.error} className="text-xs" />}
            {connectAction.error && (
              <WebErrorCard error={connectAction.error} className="text-xs" />
            )}
            {connection && (
              <div className="flex items-center gap-2 rounded-lg border bg-background px-3 py-2">
                <StatusDot colorClass={PATH_META[connection.path].dot} />
                <span className="text-xs font-medium text-foreground">
                  {t(PATH_META[connection.path].label)}
                </span>
                <span className="truncate font-mono text-xs text-muted-foreground">
                  {connection.addr}
                </span>
              </div>
            )}
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setShowInput(true)}
            disabled={!ready}
            // `focus-visible:-outline-offset-2`：这一行是通栏的，两侧紧贴 `SettingsCard`
            // 的边缘，而那张卡是 `overflow-hidden`——`.focus-ring` 往外 2px 的环左右两段
            // 会被整段裁掉，只剩上下两条横线。同一个环，只把偏移翻个号。
            className="focus-ring flex w-full items-center justify-between gap-3 border-b p-4 text-sm text-muted-foreground transition-colors last:border-b-0 hover:bg-accent/40 hover:text-foreground focus-visible:-outline-offset-2 disabled:opacity-50"
          >
            <span className="flex min-w-0 items-center gap-2">
              <Plus className="size-4 shrink-0" aria-hidden />
              <Trans>添加自定义引导节点</Trans>
            </span>
            {/*
              输入格式提示。**曾经是 `bg-foreground` + `text-background` 的实心胶囊**——
              浅色下一块纯黑、深色下一块纯白，是整张设置页对比度最高的元素，却挂在
              全页优先级最低的一行（一个收起着的次要动作）上。视觉重量与信息重量正好反着，
              而且它是这一页第四种徽标方言：同一个面板里的传输名用 `bg-primary/10 text-brand`，
              分组标题旁的计数用 `Badge variant="outline"`。

              改成 shared `Badge` 的 outline 档——DESIGN.md §5 把 outline/ghost 定义为
              「低强调标签」，这正是一个。`text-inherit` 让它跟着整行的
              `text-muted-foreground → hover:text-foreground` 一起走，
              于是这一行读起来是一个整体，而不是一行灰字外加一块黑砖。
            */}
            <Badge variant="outline" className="text-[10px] text-inherit">
              Multiaddr
            </Badge>
          </button>
        )}

        {activeCircuit && (
          <SettingsRow
            title={<Trans>你的可达地址</Trans>}
            description={
              <Trans>
                <Link href={NAV.devices.href} className="underline underline-offset-2">
                  生成邀请
                </Link>{" "}
                时会写进邀请里
              </Trans>
            }
          >
            <p className="break-all rounded-lg border bg-background px-3 py-2 font-mono text-xs text-foreground">
              {activeCircuit}
            </p>
          </SettingsRow>
        )}
      </SettingsCard>
    </SettingsSection>
  );
}

/**
 * 「默认入口」栅格里的一张卡：传输名 + 短地址 + 运行态。
 *
 * 与桌面同名栅格的差别在**第三行**：桌面那份是纯静态配置（它的 bootstrap 节点只在启动时
 * 用一次，没有持续状态可言），这里的每一条都是活的 relay 连接，所以把内核下发的
 * connecting / active / failed 直接摆出来——「状态诚实可见」是 PRODUCT.md 的原则 2，
 * 而这一栏恰恰是用户排查「为什么对方连不上我」的第一站。
 *
 * `relay === undefined` 表示这条内置地址此刻不在内核清单里：要么还没 ensure 上，
 * 要么被用户撤过。后者需要一条回头路，见 `doRestore`。
 */
function BootstrapNodeCard({
  addr,
  relay,
  busy,
  disabled,
  onDrop,
  onRestore,
}: {
  addr: string;
  relay: RelayInfoJson | undefined;
  busy: boolean;
  disabled: boolean;
  onDrop: (id: string) => void;
  onRestore: () => void;
}) {
  return (
    <div className="min-w-0 rounded-xl border bg-background/55 p-3 dark:bg-white/[0.035]">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 text-xs font-medium text-foreground">
          <ShieldCheck className="size-3.5 text-brand" aria-hidden />
          <Trans>默认</Trans>
        </span>
        {/* 传输名是专有名词，永不翻译（见 relay-helpers.ts 的 `bootstrapTransport`）。 */}
        <span className="shrink-0 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-semibold text-brand">
          {bootstrapTransport(addr)}
        </span>
      </div>

      <span className="block truncate font-mono text-[11px] text-muted-foreground">
        {truncateAddr(addr)}
      </span>

      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="min-w-0 text-[11px] text-muted-foreground">
          {relay ? <RelayStateLabel relay={relay} /> : <Trans>未连接</Trans>}
        </span>

        {relay ? (
          <DropRelayButton onDrop={() => onDrop(relay.id)} disabled={disabled || busy} />
        ) : (
          <Button
            size="xs"
            variant="ghost"
            onClick={onRestore}
            disabled={disabled}
            className="shrink-0 text-[11px]"
          >
            <Trans>重新连接</Trans>
          </Button>
        )}
      </div>

      <RelayError message={relay?.lastError ?? null} className="mt-1.5" />
    </div>
  );
}

/** 状态点 + 文案。两处清单（默认栅格 / 自定义列表）共用，字号由父级给。 */
function RelayStateLabel({ relay }: { relay: RelayInfoJson }) {
  const { t } = useLingui();
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <StatusDot colorClass={RELAY_META[relay.state].dot} pulse={relay.state === "connecting"} />
      <span className="truncate">{t(RELAY_META[relay.state].label)}</span>
    </span>
  );
}

/**
 * 末次失败原因。**内核原样下发**（`RelayInfoJson.lastError`），这里不加工——
 * 排查连不上时用户要贴的就是这一句。
 */
function RelayError({ message, className }: { message: string | null; className?: string }) {
  if (!message) return null;
  return (
    <p className={cn("break-words text-[11px] text-destructive-ink", className)}>{message}</p>
  );
}

/**
 * 撤销一条引导节点。
 *
 * **内置项也用它**——启动时会自动 ensure 全部内置地址，填错的那条会在后台无限退避重试
 * 而界面一片安静，所以每条都必须撤得掉（已修过的缺陷，别退回去）。
 */
function DropRelayButton({ onDrop, disabled }: { onDrop: () => void; disabled: boolean }) {
  const { t } = useLingui();
  return (
    <Button
      size="icon-sm"
      variant="ghost"
      aria-label={t`移除引导节点`}
      onClick={onDrop}
      disabled={disabled}
      className="shrink-0 rounded-full text-muted-foreground hover:text-destructive"
    >
      <Trash2 className="size-3.5" aria-hidden />
    </Button>
  );
}
