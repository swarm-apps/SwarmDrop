"use client";

// 引导节点：浏览器怎么被别人找到。
//
// **组件名仍是 `ConnectionPanel`**，但它管的东西已经从「relay 意图」泛化成「基础设施关系」
// ——同一条关系可以同时承担 DHT 种子与 circuit 中继两个角色，两者在内核里从第一天起就正交。
// 清单里那份读模型是 core 的 `InfraLink`，三端同一个形状（DESIGN.md 的 Node Status Contract）。
//
// ## 浏览器不监听端口，所以「可达」要自己建
//
// 桌面/移动会 listen 本地 socket，对端直接拨得到。浏览器不行，必须先经一个引导节点建立
// circuit 可达地址，那个地址才是邀请里写给对方的东西。这块的全部内容都在服务这一件事。
//
// ## 清单来自内核，不是本地记的
//
// 由 layout 的 `startInfraWatch` 单点订阅 `infra_changed()` 写进 store。**订阅不在这个面板
// 里**：设备页的配对区也要读同一份事实（判断能不能生成邀请），常驻徽章还要拿它算整体健康度，
// 绑在某一页上会让直接进设备页的用户看到一个永远禁用的「生成邀请」——见 `_lib/infra-watch.ts`。
//
// ## 没有「测试连通性」按钮，这是刻意删掉的
//
// 那颗按钮走的是 `WebNode.connect`（直连），而中继的实际用法是 reservation，两条链路不同；
// 更糟的是它对**已连接**的对端直接返回既有连接快照——于是对已经连上的内置节点**永远绿**。
// 一个不可能失败的测试比没有测试更坏。取而代之是两段：提交前由 core 同步校验形状与
// transport（零网络成本、100% 确定），提交后由收敛环给答案（测的就是后续真正会走的链路）。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import { Plus, RadioTower, ShieldCheck, Trash2 } from "lucide-react";
import Link from "next/link";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import type { InfraLinkPresentation } from "@swarmdrop/shared-view";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/cn";
import { refreshInfraLinks } from "../_lib/infra-watch";
import { getNode } from "../_lib/node-runtime";
import { NAV } from "../_lib/nav";
import {
  INFRA_LINK_STATE_LABEL,
  TONE_DOT,
  infraAddrErrorLabel,
  toInfraAddrError,
} from "../_lib/network-view";
import { preferencesActions, usePreferences } from "../_lib/preferences-store";
import {
  WEB_RELAY_HELPERS,
  WEB_RELAY_PEER_IDS,
  bootstrapPeerId,
  bootstrapTransport,
  truncateAddr,
} from "../_lib/relay-helpers";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { useInfraLinkRows, type InfraLinkRow } from "../_lib/use-network-status";
import { selectReservation, useWebNode } from "../_lib/store";
import { CopyButton } from "./copy-button";
import { SettingsCard, SettingsRow, SettingsSection } from "./settings-primitives";
import { StatusDot } from "./status-dot";

/** 添加失败时那句话往哪儿说。见 `reportAddError`。 */
type AddErrorChannel = "inline" | "toast";

export function ConnectionPanel() {
  const { t } = useLingui();
  const nodeStatus = useWebNode((s) => s.status);
  const activeCircuit = useWebNode(selectReservation);
  const rows = useInfraLinkRows();
  // selector 只返回 store 里的稳定数组引用，不派生——规则 B。
  const removedBuiltins = usePreferences((s) => s.infraNodes.removed);

  const [addr, setAddr] = useState("");
  /** 添加入口默认收起：这是极低频动作，常驻的输入框 + 两颗按钮占了整张卡三分之一。 */
  const [showInput, setShowInput] = useState(false);
  /** 提交前校验的结果。**是描述符不是字符串**，展开在渲染处。 */
  const [addError, setAddError] = useState<MessageDescriptor | null>(null);
  const dropAction = useKeyedAsyncAction();
  const addAction = useKeyedAsyncAction();

  const ready = nodeStatus === "running";

  /**
   * 把内核下发的运行态切成「内置 / 自定义」两半。
   *
   * 判据是 **peer id**：内置项的 id 从 `WEB_RELAY_HELPERS` 的 multiaddr 尾部解出来，
   * 与 `InfraLink.peerId` 比对。**不比地址**——同一个节点可以有多条路径，内核会把它们
   * 合并进同一条关系。
   */
  const customRows = useMemo(
    () => rows.filter((r) => !WEB_RELAY_PEER_IDS.has(r.link.peerId)),
    [rows],
  );
  const rowById = useMemo(
    () => new Map(rows.map((r) => [r.link.peerId, r])),
    [rows],
  );

  const closeInput = () => {
    setShowInput(false);
    setAddr("");
    setAddError(null);
  };

  /**
   * 校验失败往哪儿说。
   *
   * 两条添加路径的差别只在**输入框还在不在**：手打那条留着输入框，错误就地内联、用户改一个
   * 字符就能重试；「重新连接」那条根本没有输入框（用户点的是一颗按钮），内联提示会写进一支
   * 没有挂载的 JSX——此前它就是这样**什么反应都没有**：无 toast、无内联、无状态变化，
   * 一颗点了不动的按钮。而这条路径真的会失败：内置清单经
   * `NEXT_PUBLIC_SWARMDROP_WEB_RELAY_HELPERS` 注入，塞一条 `/tcp/` 进去浏览器压根没装配那种
   * transport，`unsupportedTransport` 当场拦下。
   */
  const reportAddError: Record<AddErrorChannel, (label: MessageDescriptor) => void> = {
    inline: setAddError,
    toast: (label) => toast.error(t(label)),
  };

  /**
   * 添加一条引导节点：**先改内核、成功了再写持久化**。
   *
   * 顺序不能反。反过来则一条过不了校验的地址会留在偏好里，此后每次启动都被回放一遍、
   * 每次都失败——而用户在界面上根本看不到它（它从没进过内核的清单）。
   *
   * `channel` 由调用方给、**没有默认值**：新增一个添加入口时必须显式回答「这条路径上
   * 用户看得到内联提示吗」，答错就是上面那颗点了不动的按钮。
   */
  const doAdd = (input: string, channel: AddErrorChannel) => {
    const node = getNode();
    const trimmed = input.trim();
    if (!node || !trimmed) return;
    setAddError(null);
    void addAction.run(trimmed, async () => {
      try {
        node.infra_ensure(trimmed);
      } catch (e) {
        const addrError = toInfraAddrError(e);
        // 校验失败是**输入的问题**，说的是那串地址哪里不对（内联时还留住用户刚打的那串）；
        // 其余（不该发生的运行时异常）走 toast，它不该假装成一条输入校验。
        if (addrError) {
          reportAddError[channel](infraAddrErrorLabel(addrError));
        } else {
          toast.error(t`添加引导节点失败`);
        }
        throw e;
      }
      preferencesActions.addInfraNode(trimmed);
      // 意图侧的增删不产生 relay 事件，补一次快照，否则新加的那条要等收敛环跑起来才出现。
      refreshInfraLinks(node);
      closeInput();
      // 不 await `until_active`：登记是**常驻意图**，后台会持续收敛。挂在这里等 30 秒
      // 只会让按钮转半天，而清单里那一条已经在显示「正在连接」了。
      toast.success(t`已添加引导节点，正在连接…`);
    });
  };

  /**
   * 把撤掉的内置节点重新登记回去。
   *
   * 内置项能撤（理由见界面上那段注释），但撤了之后没有回头路就成了单向门——用户得自己
   * 从文档里把那串 multiaddr 找回来手打。走的是与「添加」同一条路径（登记幂等 + 偏好侧
   * 把它从 removed 里划掉），所以这里只是换个入口。
   *
   * 校验失败走 toast：这条路径上没有输入框可挂内联提示（见 `reportAddError`）。
   */
  const doRestore = (helperAddr: string) => doAdd(helperAddr, "toast");

  const doDrop = (peerId: string) => {
    const node = getNode();
    if (!node) return;
    void dropAction.run(peerId, async () => {
      try {
        await node.infra_drop(peerId);
      } catch (e) {
        toast.error(t`移除引导节点失败`);
        throw e;
      }
      // 同 `doAdd`：先内核后持久化。
      preferencesActions.forgetInfraNode(peerId);
      refreshInfraLinks(node);
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
          {customRows.length > 0 ? (
            <Trans>自定义 {customRows.length}</Trans>
          ) : (
            <Trans>默认</Trans>
          )}
        </Badge>
      }
    >
      <SettingsCard>
        {/* ── 默认入口 ─────────────────────────────────────────────────────
            内置清单（`WEB_RELAY_HELPERS`）此前只当输入框的 placeholder 用，界面上
            根本看不到「默认连的是哪几台」。桌面把这块摆成一个只读栅格，这里照做。

            **但不加「只读」徽标**：桌面的默认节点确实删不掉，这里的能删——启动时会自动
            登记全部内置项，填错的那条会在后台无限退避重试而界面一片安静，所以每条都
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
              const peerId = bootstrapPeerId(helperAddr);
              const row = peerId === null ? undefined : rowById.get(peerId);
              return (
                <BootstrapNodeCard
                  key={helperAddr}
                  addr={helperAddr}
                  row={row}
                  removed={peerId !== null && removedBuiltins.includes(peerId)}
                  busy={
                    (peerId !== null && dropAction.isPending(peerId)) ||
                    addAction.isPending(helperAddr)
                  }
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

          {customRows.length > 0 ? (
            <ul className="mt-3 grid gap-2">
              {customRows.map((row) => (
                <li
                  key={row.link.peerId}
                  className="flex min-w-0 items-start justify-between gap-3 rounded-xl border bg-background/55 p-3 dark:bg-white/[0.035]"
                >
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-medium text-foreground">
                      <InfraStateLabel presentation={row.presentation} />
                    </p>
                    {/* 地址与身份都是机器真值 → mono（Mono Truth Rule）。 */}
                    <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                      {row.link.addrs[0]
                        ? truncateAddr(row.link.addrs[0])
                        : `…${row.link.peerId.slice(-8)}`}
                    </p>
                    <InfraLinkDetail
                      detail={row.presentation.detail}
                      copyLabel={t`复制错误信息`}
                    />
                  </div>
                  {/* 移除入口按 `removable` 门控：**纯**自动来源（局域网协助 / identify
                      学到的）撤了会断开与该节点的全部连接——而那可能是一台正在传文件的
                      已配对设备，何况它下次 identify 就被原样登记回来。点了没反应，还把
                      传输搞挂。

                      判据在 core（`infra/link.rs`），是「sources **含有** HostConfigured」。
                      曾写成「全是」，于是用户自己加的这一条一旦连上就永久删不掉——`upsert`
                      对 sources 是累加，而对端是 bootstrap agent 时 `learn_candidate` 会补
                      一条 `Learned`。别在这一侧另立判据把它绕回来。 */}
                  {row.link.removable && (
                    <DropInfraButton
                      onDrop={() => doDrop(row.link.peerId)}
                      disabled={!ready || dropAction.isPending(row.link.peerId)}
                    />
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-3 rounded-xl border border-dashed bg-background/35 px-3 py-4 text-xs leading-5 text-muted-foreground dark:bg-white/[0.025]">
              <Trans>当前使用内置引导节点。</Trans>
            </p>
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
              onChange={(e) => {
                setAddr(e.target.value);
                // 用户开始改了就把上一次的错误撤掉——留着它会让人以为改了也没用。
                if (addError) setAddError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  doAdd(addr, "inline");
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
                onClick={() => doAdd(addr, "inline")}
                disabled={!ready || !addr.trim() || addAction.isPending(addr.trim())}
              >
                {addAction.isPending(addr.trim()) ? (
                  <Trans>添加中…</Trans>
                ) : (
                  <Trans>添加</Trans>
                )}
              </Button>
              <Button size="sm" variant="ghost" onClick={closeInput}>
                <Trans>取消</Trans>
              </Button>
            </div>
            {addError && (
              <p className="break-words text-xs leading-5 text-destructive-ink">
                {t(addError)}
              </p>
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
              输入格式提示。用 shared `Badge` 的 outline 档——DESIGN.md §5 把 outline/ghost
              定义为「低强调标签」，这正是一个。`text-inherit` 让它跟着整行的
              `text-muted-foreground → hover:text-foreground` 一起走，于是这一行读起来是
              一个整体，而不是一行灰字外加一块黑砖。
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

/** 「未在清单里」这一档的文案：撤过的说「已移除」，其余说「未连接」。 */
const ABSENT_LABEL: Record<"removed" | "absent", MessageDescriptor> = {
  removed: msg`已移除`,
  absent: msg`未连接`,
};

/**
 * 「默认入口」栅格里的一张卡：传输名 + 短地址 + 运行态。
 *
 * 与桌面同名栅格的差别在**第三行**：这里每一条都摆出内核下发的实时状态——「状态诚实可见」
 * 是 PRODUCT.md 的原则 2，而这一栏恰恰是用户排查「为什么对方连不上我」的第一站。
 * （桌面那份同样持有活的 reservation，只是它此前没把状态摆出来；不是「纯静态配置」。）
 *
 * `row === undefined` 表示这条内置地址此刻不在内核清单里：要么还没登记上，要么被用户撤过。
 * 后者需要一条回头路，见 `doRestore`。
 */
function BootstrapNodeCard({
  addr,
  row,
  removed,
  busy,
  disabled,
  onDrop,
  onRestore,
}: {
  addr: string;
  row: InfraLinkRow | undefined;
  removed: boolean;
  busy: boolean;
  disabled: boolean;
  onDrop: (peerId: string) => void;
  onRestore: () => void;
}) {
  const { t } = useLingui();
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
          {row ? (
            <InfraStateLabel presentation={row.presentation} />
          ) : (
            t(ABSENT_LABEL[removed ? "removed" : "absent"])
          )}
        </span>

        <BootstrapNodeAction
          row={row}
          disabled={disabled || busy}
          onDrop={onDrop}
          onRestore={onRestore}
        />
      </div>

      <InfraLinkDetail
        detail={row?.presentation.detail ?? null}
        copyLabel={t`复制错误信息`}
        className="mt-1.5"
      />
    </div>
  );
}

/**
 * 内置卡右下角那一颗按钮，三种情形各一种：
 *
 *   不在清单里     → 「重新连接」（撤过之后必须有回头路，否则是个单向门）
 *   在清单且可移除 → 移除
 *   在清单不可移除 → 什么都不给（自动来源，撤了会被原样登记回来，还可能掐断传输）
 *
 * 写成组件而不是 JSX 里的嵌套三元：三档各有各的理由，早返回才写得下那三行注释。
 */
function BootstrapNodeAction({
  row,
  disabled,
  onDrop,
  onRestore,
}: {
  row: InfraLinkRow | undefined;
  disabled: boolean;
  onDrop: (peerId: string) => void;
  onRestore: () => void;
}) {
  if (!row) {
    return (
      <Button
        size="xs"
        variant="ghost"
        onClick={onRestore}
        disabled={disabled}
        className="shrink-0 text-[11px]"
      >
        <Trans>重新连接</Trans>
      </Button>
    );
  }
  if (!row.link.removable) return null;
  return <DropInfraButton onDrop={() => onDrop(row.link.peerId)} disabled={disabled} />;
}

/** 状态点 + 文案。两处清单（默认栅格 / 自定义列表）共用，字号由父级给。 */
function InfraStateLabel({ presentation }: { presentation: InfraLinkPresentation }) {
  const { t } = useLingui();
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <StatusDot
        colorClass={TONE_DOT[presentation.tone]}
        pulse={presentation.state === "settling"}
      />
      <span className="truncate">{t(INFRA_LINK_STATE_LABEL[presentation.state])}</span>
    </span>
  );
}

/**
 * 末次失败原因。**内核原样下发，永不翻译**——排查连不上时用户要贴进 issue、跟日志比对的
 * 就是这一句，翻过的串失去这个用途。
 *
 * 配一颗复制按钮：一串长机器文本不可选中却长得像可点，正好违反复制可供性那条规矩
 * （`theme-and-styling.md`）。
 */
function InfraLinkDetail({
  detail,
  copyLabel,
  className,
}: {
  detail: string | null;
  copyLabel: string;
  className?: string;
}) {
  if (!detail) return null;
  return (
    <div className={cn("flex items-start gap-1", className)}>
      <p className="min-w-0 flex-1 break-words text-[11px] text-destructive-ink">{detail}</p>
      <CopyButton key={detail} value={detail} label={copyLabel} className="h-6 px-1.5" />
    </div>
  );
}

/**
 * 撤销一条引导节点。
 *
 * **内置项也用它**——启动时会自动登记全部内置地址，填错的那条会在后台无限退避重试
 * 而界面一片安静，所以每条都必须撤得掉（已修过的缺陷，别退回去）。
 */
function DropInfraButton({ onDrop, disabled }: { onDrop: () => void; disabled: boolean }) {
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
