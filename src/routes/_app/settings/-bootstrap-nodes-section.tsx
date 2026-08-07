/**
 * BootstrapNodesSection
 * 设置页「引导节点」区域 — 管理默认 + 自定义引导节点
 */

import { useState } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { Copy, Plus, Trash2, RadioTower, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useNodeRestart } from "@/hooks/use-node-restart";
import { DESKTOP_BOOTSTRAP_NODES } from "@/lib/bootstrap-nodes";
import { copyText } from "@/lib/clipboard";
import { toast } from "sonner";
import {
  NodeRestartBanner,
  SettingsCard,
  SettingsSection,
} from "./-settings-primitives";

/** 简单的 Multiaddr 格式校验：必须包含 /p2p/ 且以 / 开头 */
function isValidMultiaddr(addr: string): boolean {
  return addr.startsWith("/") && addr.includes("/p2p/");
}

/**
 * 截断 Multiaddr 用于显示。与 Web 端 `docs/app/app/_lib/relay-helpers.ts` 的同名函数同形。
 *
 * `p2pIdx > 30` 时**必须补省略号**：本仓的内置地址正好命中，不补的话输出是
 * `/ip4/47.115.172.218/udp/4003/w/p2p/12D3Ko…1utep` —— certhash 整段消失、切口处毫无提示，
 * 看起来像一条完整但写错的 multiaddr，而用户会照着它去 issue 里贴。
 */
function truncateAddr(addr: string): string {
  if (addr.length <= 60) return addr;
  // 保留协议头和末尾 peer id
  const p2pIdx = addr.indexOf("/p2p/");
  if (p2pIdx === -1) return `${addr.slice(0, 30)}…${addr.slice(-20)}`;
  const prefix = p2pIdx > 30 ? `${addr.slice(0, 30)}…` : addr.slice(0, p2pIdx);
  const peerId = addr.slice(p2pIdx + 5);
  const shortPeerId = peerId.length > 12
    ? `${peerId.slice(0, 6)}…${peerId.slice(-6)}`
    : peerId;
  return `${prefix}/p2p/${shortPeerId}`;
}

/**
 * 传输名。**是专有名词，永不翻译**（DESIGN.md 的 Device Card Contract）。
 *
 * `webrtc-direct` 标成 `WebRTC Direct` 而不是 `WebRTC`：契约里这是两种传输
 * （前者要打洞与信令，后者直接拨公网裸 IP），混称会让人以为换个节点也能用打洞地址。
 * 判定按**特异性从高到低**——`/webrtc-direct` 必须排在 `/webrtc` 前面，否则永远匹配不到。
 */
function getTransportLabel(addr: string): string {
  if (addr.includes("/webrtc-direct")) return "WebRTC Direct";
  if (addr.includes("/webrtc")) return "WebRTC";
  if (addr.includes("/wss")) return "WSS";
  if (addr.includes("/ws")) return "WebSocket";
  if (addr.includes("/quic")) return "QUIC";
  if (addr.includes("/tcp/")) return "TCP";
  return "P2P";
}

export function BootstrapNodesSection() {
  const { t } = useLingui();
  const customBootstrapNodes = usePreferencesStore((s) => s.customBootstrapNodes);
  const addBootstrapNode = usePreferencesStore((s) => s.addBootstrapNode);
  const removeBootstrapNode = usePreferencesStore((s) => s.removeBootstrapNode);
  const { restarting, markRestartNeeded, restart, showBanner } = useNodeRestart();

  const [inputValue, setInputValue] = useState("");
  const [showInput, setShowInput] = useState(false);

  function handleAdd() {
    const addr = inputValue.trim();
    if (!addr) return;

    if (!isValidMultiaddr(addr)) {
      toast.error(t(msg`无效的 Multiaddr 地址`), {
        description: t(msg`地址需以 / 开头且包含 /p2p/ 部分`),
      });
      return;
    }

    if (
      customBootstrapNodes.includes(addr) ||
      DESKTOP_BOOTSTRAP_NODES.includes(addr)
    ) {
      toast.error(t(msg`该节点已存在`));
      return;
    }

    addBootstrapNode(addr);
    setInputValue("");
    setShowInput(false);
    markRestartNeeded();
  }

  function handleRemove(addr: string) {
    removeBootstrapNode(addr);
    markRestartNeeded();
  }

  /**
   * 复制的是**完整** `addr`，不是屏幕上那串 `truncateAddr(addr)`——后者中间被省略号
   * 吃掉一段，贴到别处就是一条写错的 multiaddr。同理 `title` 也给全值：这两处是设置页
   * 里唯一拿不到完整引导地址的地方（截断 + CSS truncate 双重遮蔽）。
   */
  async function handleCopyAddr(addr: string) {
    try {
      await copyText(addr);
      toast.success(t(msg`引导节点地址已复制`));
    } catch {
      toast.error(t(msg`复制失败`));
    }
  }

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
        {/* 默认节点 */}
        <div className="border-b border-border/60 p-4">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <span className="text-sm font-medium text-foreground">
                <Trans>默认入口</Trans>
              </span>
              <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
                <Trans>内置节点用于首次发现网络，通常无需调整。</Trans>
              </span>
            </div>
            <Badge variant="secondary" className="shrink-0 rounded-full text-[10px]">
              <Trans>只读</Trans>
            </Badge>
          </div>

          <div className="mt-3 grid gap-2 sm:grid-cols-2">
            {DESKTOP_BOOTSTRAP_NODES.map((addr) => (
              <button
                key={addr}
                type="button"
                onClick={() => handleCopyAddr(addr)}
                title={addr}
                className="group min-w-0 rounded-xl border border-border/70 bg-background/55 p-3 text-left transition-colors hover:bg-accent/40 dark:bg-white/[0.035]"
              >
                <div className="mb-2 flex items-center justify-between gap-2">
                  <span className="flex items-center gap-1.5 text-xs font-medium text-foreground">
                    <ShieldCheck className="size-3.5 text-brand" />
                    <Trans>默认</Trans>
                  </span>
                  <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-semibold text-brand dark:bg-primary/10">
                    {getTransportLabel(addr)}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground">
                    {truncateAddr(addr)}
                  </span>
                  <Copy className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
                </div>
              </button>
            ))}
          </div>
        </div>

        {/* 自定义节点 */}
        <div className="border-b border-border/60 p-4">
          <span className="text-sm font-medium text-foreground">
            <Trans>自定义节点</Trans>
          </span>
          <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
            <Trans>仅在需要接入私有或备用网络时添加。</Trans>
          </span>

          {customBootstrapNodes.length > 0 ? (
            <div className="mt-3 grid gap-2">
              {customBootstrapNodes.map((addr) => (
                <div
                  key={addr}
                  className="flex min-w-0 items-center justify-between gap-3 rounded-xl border border-border/70 bg-background/55 p-3 dark:bg-white/[0.035]"
                >
                  {/* 这行右侧已有删除按钮，整行不能做成 button（嵌套 button 非法），
                      故只把地址本身包成可复制按钮。 */}
                  <button
                    type="button"
                    onClick={() => handleCopyAddr(addr)}
                    title={addr}
                    className="group flex min-w-0 flex-1 items-center gap-2 text-left"
                  >
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">
                      {truncateAddr(addr)}
                    </span>
                    <Copy className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
                  </button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7 shrink-0 rounded-full text-muted-foreground hover:text-destructive"
                    onClick={() => handleRemove(addr)}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          ) : (
            <div className="mt-3 rounded-xl border border-dashed border-border/80 bg-background/35 px-3 py-4 text-xs leading-5 text-muted-foreground dark:bg-white/[0.025]">
              <Trans>当前使用内置引导节点。</Trans>
            </div>
          )}
        </div>

        {/* 添加输入框 */}
        {showInput ? (
          <div className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center">
            <Input
              value={inputValue}
              onChange={(e) => setInputValue(e.target.value)}
              placeholder={t(msg`输入 Multiaddr 地址，如 /ip4/.../p2p/...`)}
              className="h-10 flex-1 rounded-xl font-mono text-xs"
              onKeyDown={(e) => {
                if (e.key === "Enter") handleAdd();
                if (e.key === "Escape") {
                  setShowInput(false);
                  setInputValue("");
                }
              }}
              autoFocus
            />
            <div className="flex shrink-0 items-center gap-2">
              <Button size="sm" className="rounded-full" onClick={handleAdd}>
                <Trans>添加</Trans>
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="rounded-full"
                onClick={() => {
                  setShowInput(false);
                  setInputValue("");
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
            className="flex w-full items-center justify-between gap-3 p-4 text-sm text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground"
          >
            <span className="flex min-w-0 items-center gap-2">
              <Plus className="size-4" />
              <Trans>添加自定义引导节点</Trans>
            </span>
            <span className="rounded-full bg-foreground px-2.5 py-1 text-[11px] font-medium text-background">
              Multiaddr
            </span>
          </button>
        )}
      </SettingsCard>

      {showBanner && (
        <NodeRestartBanner
          message={<Trans>引导节点已变更，需重启节点生效</Trans>}
          restarting={restarting}
          onRestart={restart}
        />
      )}
    </SettingsSection>
  );
}
