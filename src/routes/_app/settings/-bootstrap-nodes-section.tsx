/**
 * BootstrapNodesSection
 * 设置页「引导节点」区域 —— 诊断层的**可编辑版本**。
 *
 * 三件事与改动前不同：
 *
 * 1. **逐条带状态**（状态点+词 / 归因「来源 · 范围 · 角色」/ 原样 `lastError` + 复制）。此前这里只有一份
 *    静态地址清单，用户加完一条完全看不出它有没有连上。
 * 2. **增删当场生效，不需要重启节点**（`addInfraNode` / `removeInfraNode` 走意图 IPC，
 *    收敛环最迟 1s 起第一轮）。顺序是**先改内核成功、再写偏好**——反过来会在内核拒绝
 *    时留下一条只存在于偏好里的幽灵节点。
 * 3. **提交前校验在后端**（`validateInfraAddr`）。规则里有两条要认识内核事实（合法 peer id
 *    形状、本端点实际装配了哪些传输），前端复制一份必然漂移。
 *
 * 移除入口按 `InfraLink.removable` 门控：自动发现来源（mDNS / learned）的节点撤销会
 * 断开与它的全部连接（含在途传输），而局域网协助节点本身可能就是一台正在传文件的已配对
 * 设备；何况下次 identify 会把它原样登记回来——点了没反应，还把传输搞挂。
 * 内置默认节点另有一层门控（见 `isBuiltIn`）。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { Copy, Plus, RadioTower, ShieldCheck, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useNodeHealth } from "@/hooks/use-node-health";
import {
  deriveInfraLinkState,
  INFRA_ADDR_ERROR_TITLE,
  INFRA_LINK_WORD,
  toInfraLinkView,
  TONE_DOT,
} from "@/lib/node-status";
import {
  InfraLinkAttribution,
  InfraLinkError,
} from "@/components/network/infra-link-parts";
import {
  transportFromAddr,
  transportLabel,
  truncateAddr,
} from "@swarmdrop/shared-view";
import { DESKTOP_BOOTSTRAP_NODES } from "@/lib/bootstrap-nodes";
import { copyText } from "@/lib/clipboard";
import { getErrorMessage } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { commands, type InfraAddrError, type InfraLink } from "@/lib/bindings";
import { toast } from "sonner";
import { SettingsCard, SettingsSection } from "./-settings-primitives";

/** 内置地址里出现过这个 peer id ⇒ 它是默认入口。 */
function isBuiltIn(peerId: string): boolean {
  return DESKTOP_BOOTSTRAP_NODES.some((addr) =>
    addr.includes(`/p2p/${peerId}`),
  );
}

export function BootstrapNodesSection() {
  const { t } = useLingui();
  const customBootstrapNodes = usePreferencesStore(
    (s) => s.customBootstrapNodes,
  );
  const addBootstrapNode = usePreferencesStore((s) => s.addBootstrapNode);
  const removeBootstrapNode = usePreferencesStore((s) => s.removeBootstrapNode);
  const { links, lifecycle, nowMs } = useNodeHealth();
  const isRunning = lifecycle === "running";

  const [inputValue, setInputValue] = useState("");
  const [showInput, setShowInput] = useState(false);
  const [rejection, setRejection] = useState<InfraAddrError | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [supported, setSupported] = useState<string[]>([]);

  // 「本端支持哪些传输」是**运行时事实**（打洞传输按配置装配），不能照平台猜。
  useEffect(() => {
    if (!showInput || !isRunning) return;
    let cancelled = false;
    commands.supportedTransports().then(
      (kinds) => {
        if (!cancelled) setSupported(kinds);
      },
      () => {
        /* 拿不到就不显示这行提示，不值得为它弹错误 */
      },
    );
    return () => {
      cancelled = true;
    };
  }, [showInput, isRunning]);

  // 提交前校验：边打边给内联错误。零网络往返，所以走一次防抖就够。
  useEffect(() => {
    const addr = inputValue.trim();
    if (!addr || !isRunning) {
      setRejection(null);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      commands.validateInfraAddr(addr).then(
        (reason) => {
          if (!cancelled) setRejection(reason);
        },
        () => {
          /* 节点在这一刻停了；提交时会再报一次，这里不抢话 */
        },
      );
    }, 350);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [inputValue, isRunning]);

  const handleAdd = useCallback(async () => {
    const addr = inputValue.trim();
    if (!addr) return;
    setSubmitting(true);
    try {
      // 校验 + 登记在后端一次完成：拆成两次 IPC 会在中间留一个候选表可能已变的窗口。
      const reason = await commands.addInfraNode(addr);
      if (reason) {
        setRejection(reason);
        return;
      }
    } catch (err) {
      toast.error(t(msg`添加引导节点失败`), {
        description: getErrorMessage(err),
      });
      return;
    } finally {
      setSubmitting(false);
    }

    // 内核已经收下，再落持久化。反过来写会在内核拒绝时留下一条幽灵节点。
    addBootstrapNode(addr);
    setInputValue("");
    setRejection(null);
    setShowInput(false);
    toast.success(t(msg`引导节点已添加`), {
      description: t(msg`无需重启节点，稍后即可看到它的连接状态。`),
    });
  }, [inputValue, addBootstrapNode, t]);

  const handleRemoveLink = useCallback(
    async (link: InfraLink) => {
      try {
        await commands.removeInfraNode(link.peerId);
      } catch (err) {
        toast.error(t(msg`移除引导节点失败`), {
          description: getErrorMessage(err),
        });
        return;
      }
      for (const addr of customBootstrapNodes) {
        if (addr.includes(`/p2p/${link.peerId}`)) removeBootstrapNode(addr);
      }
      toast.success(t(msg`引导节点已移除`));
    },
    [customBootstrapNodes, removeBootstrapNode, t],
  );

  /**
   * 复制的是**完整** `addr`，不是屏幕上那串 `truncateAddr(addr)`——后者中间被省略号
   * 吃掉一段，贴到别处就是一条写错的 multiaddr。同理 `title` 也给全值。
   */
  const handleCopyAddr = useCallback(
    async (addr: string) => {
      try {
        await copyText(addr);
        toast.success(t(msg`引导节点地址已复制`));
      } catch {
        toast.error(t(msg`复制失败`));
      }
    },
    [t],
  );

  return (
    <SettingsSection
      title={<Trans>引导节点</Trans>}
      icon={RadioTower}
      aside={
        <Badge variant="outline" className="rounded-full text-[10px]">
          {customBootstrapNodes.length > 0 ? (
            <Trans>自定义 {customBootstrapNodes.length}</Trans>
          ) : (
            <Trans>默认</Trans>
          )}
        </Badge>
      }
      fill
    >
      <SettingsCard fill>
        <div className="border-b border-border/60 p-4">
          <span className="text-sm font-medium text-foreground">
            <Trans>连接状态</Trans>
          </span>
          <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
            <Trans>
              引导节点帮你找到其他网络里的设备。增删当场生效，不需要重启节点。
            </Trans>
          </span>

          {isRunning ? (
            <LiveLinkList
              links={links}
              nowMs={nowMs}
              onCopy={handleCopyAddr}
              onRemove={handleRemoveLink}
            />
          ) : (
            <OfflineList
              custom={customBootstrapNodes}
              onCopy={handleCopyAddr}
              onRemove={removeBootstrapNode}
            />
          )}
        </div>

        {showInput ? (
          <div className="flex flex-col gap-3 p-4">
            <Input
              value={inputValue}
              onChange={(e) => setInputValue(e.target.value)}
              placeholder={t(msg`输入 Multiaddr 地址，如 /ip4/.../p2p/...`)}
              className="h-10 rounded-xl font-mono text-xs"
              aria-invalid={rejection !== null}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleAdd();
                if (e.key === "Escape") {
                  setShowInput(false);
                  setInputValue("");
                  setRejection(null);
                }
              }}
              autoFocus
            />

            {rejection && <RejectionNotice rejection={rejection} />}

            {supported.length > 0 && !rejection && (
              <span className="text-[11px] leading-5 text-muted-foreground">
                <Trans>
                  本端支持的传输：{supported.map((t) => transportLabel(t) ?? t).join(" · ")}
                </Trans>
              </span>
            )}

            <div className="flex items-center gap-2">
              <Button
                size="sm"
                className="rounded-full"
                onClick={handleAdd}
                disabled={submitting || rejection !== null || !inputValue.trim()}
              >
                <Trans>添加</Trans>
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="rounded-full"
                onClick={() => {
                  setShowInput(false);
                  setInputValue("");
                  setRejection(null);
                }}
              >
                <Trans>取消</Trans>
              </Button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setShowInput(true)}
            disabled={!isRunning}
            className="flex w-full items-center justify-between gap-3 p-4 text-sm text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground disabled:pointer-events-none disabled:opacity-60"
          >
            <span className="flex min-w-0 items-center gap-2">
              <Plus className="size-4" />
              {isRunning ? (
                <Trans>添加自定义引导节点</Trans>
              ) : (
                // 校验要问内核「本端装配了哪些传输」，节点没跑就问不出来。与其在前端
                // 复制一份必然漂移的规则，不如把入口关掉并说清为什么。
                <Trans>启动节点后可添加引导节点</Trans>
              )}
            </span>
            <span className="rounded-full bg-primary/10 px-2.5 py-1 text-[11px] font-medium text-brand">
              Multiaddr
            </span>
          </button>
        )}
      </SettingsCard>
    </SettingsSection>
  );
}

/** 节点运行中：逐条来自内核的 `InfraLink`，含默认 / 自定义 / 自动发现全部来源。 */
function LiveLinkList({
  links,
  nowMs,
  onCopy,
  onRemove,
}: {
  links: InfraLink[];
  nowMs: number;
  onCopy: (addr: string) => void;
  onRemove: (link: InfraLink) => void;
}) {
  const { t } = useLingui();

  if (links.length === 0) {
    return (
      <div className="mt-3 rounded-xl border border-dashed border-border/80 bg-background/35 px-3 py-4 text-xs leading-5 text-muted-foreground dark:bg-white/[0.025]">
        <Trans>还没有引导节点。</Trans>
      </div>
    );
  }

  return (
    <div className="mt-3 grid gap-2" data-testid="infra-link-rows">
      {links.map((link) => {
        const state = deriveInfraLinkState(toInfraLinkView(link), nowMs);
        const builtIn = isBuiltIn(link.peerId);
        const addr = link.addrs[0] ?? "";
        return (
          <div
            key={link.peerId}
            className="flex min-w-0 flex-col gap-2 rounded-xl border border-border/70 bg-background/55 p-3 dark:bg-white/[0.035]"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="flex min-w-0 items-center gap-1.5">
                <span
                  className={cn(
                    "size-2 shrink-0 rounded-full",
                    TONE_DOT[state.tone],
                  )}
                />
                <span className="truncate text-xs font-medium text-foreground">
                  {t(INFRA_LINK_WORD[state.state])}
                </span>
                {builtIn && (
                  <ShieldCheck
                    className="size-3.5 shrink-0 text-brand"
                    aria-label={t(msg`默认入口`)}
                  />
                )}
              </span>
              <span className="flex shrink-0 items-center gap-1">
                {addr && (
                  <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-semibold text-brand">
                    {transportFromAddr(addr)}
                  </span>
                )}
              </span>
            </div>

            {/* 归因：来源 · 范围 · 角色。此前这里只有来源，于是手填的局域网节点与公网
                节点长得一模一样，而 scope 正是「你关了公网可达性」那一档的判据。 */}
            <InfraLinkAttribution link={link} />

            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => onCopy(addr)}
                title={addr}
                disabled={!addr}
                className="group flex min-w-0 flex-1 items-center gap-2 text-left disabled:pointer-events-none"
              >
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">
                  {addr ? truncateAddr(addr) : link.peerId}
                </span>
                {addr && (
                  <Copy className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
                )}
              </button>
              {/* 内置默认节点即使 `removable` 也不给移除：它是常量清单，下次启动会原样
                  回来。给一个「点了这次管用、重启就复活」的按钮比不给更糟。 */}
              {link.removable && !builtIn && (
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t(msg`移除引导节点`)}
                  title={t(msg`移除引导节点`)}
                  className="size-7 shrink-0 rounded-full text-muted-foreground hover:text-destructive"
                  onClick={() => onRemove(link)}
                >
                  <Trash2 className="size-3.5" />
                </Button>
              )}
            </div>

            {/* `lastError` 原样不翻译：排查时用户要贴的就是这一句。复制走共享零件——
                此前它借用 `onCopy`（父组件的「引导节点地址已复制」），确认语说的是另一件事。 */}
            {state.detail && <InfraLinkError detail={state.detail} />}
          </div>
        );
      })}
    </div>
  );
}

/**
 * 节点未运行：内核里什么都没有，只能把「下次启动会用哪些地址」摆出来。
 *
 * 刻意**不画状态点**——没有观测源时给一个灰点，和「已知它是灰的」长得一模一样。
 */
function OfflineList({
  custom,
  onCopy,
  onRemove,
}: {
  custom: string[];
  onCopy: (addr: string) => void;
  onRemove: (addr: string) => void;
}) {
  const { t } = useLingui();
  const rows = useMemo(
    () => [
      ...DESKTOP_BOOTSTRAP_NODES.map((addr) => ({ addr, builtIn: true })),
      ...custom.map((addr) => ({ addr, builtIn: false })),
    ],
    [custom],
  );

  return (
    <div className="mt-3 grid gap-2">
      <span className="text-xs leading-5 text-muted-foreground">
        <Trans>节点未运行，下面是下次启动时会用的地址。</Trans>
      </span>
      {rows.map(({ addr, builtIn }) => (
        <div
          key={addr}
          className="flex min-w-0 items-center gap-2 rounded-xl border border-border/70 bg-background/55 p-3 dark:bg-white/[0.035]"
        >
          {builtIn && <ShieldCheck className="size-3.5 shrink-0 text-brand" />}
          <button
            type="button"
            onClick={() => onCopy(addr)}
            title={addr}
            className="group flex min-w-0 flex-1 items-center gap-2 text-left"
          >
            <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">
              {truncateAddr(addr)}
            </span>
            <Copy className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
          </button>
          {!builtIn && (
            <Button
              variant="ghost"
              size="icon"
              aria-label={t(msg`移除引导节点`)}
              title={t(msg`移除引导节点`)}
              className="size-7 shrink-0 rounded-full text-muted-foreground hover:text-destructive"
              onClick={() => onRemove(addr)}
            >
              <Trash2 className="size-3.5" />
            </Button>
          )}
        </div>
      ))}
    </div>
  );
}

/**
 * 提交前校验的内联报错。
 *
 * **按变体渲染，不匹配错误串**：后端 `InfraAddrError` 加一个变体，`INFRA_ADDR_ERROR_TITLE`
 * 这张表编译期就红。两个带数据的变体各自把数据说出来——那正是它们携带数据的理由。
 */
function RejectionNotice({ rejection }: { rejection: InfraAddrError }) {
  const { t } = useLingui();
  return (
    <div
      data-testid="infra-addr-rejection"
      className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2"
    >
      <span className="block text-xs font-medium text-destructive-ink">
        {t(INFRA_ADDR_ERROR_TITLE[rejection.kind])}
      </span>
      {rejection.kind === "malformed" && (
        <code className="mt-1 block break-all font-mono text-[11px] leading-relaxed text-muted-foreground">
          {rejection.detail}
        </code>
      )}
      {rejection.kind === "unsupportedTransport" && (
        <span className="mt-1 block text-[11px] leading-5 text-muted-foreground">
          <Trans>
            地址用的是 {transportLabel(rejection.transport) ?? rejection.transport}，本端支持的是{" "}
            {rejection.supported.map((t) => transportLabel(t) ?? t).join(" · ")}。
          </Trans>
        </span>
      )}
    </div>
  );
}
