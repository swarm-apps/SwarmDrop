/**
 * BootstrapNodesSection
 * 设置页「高级引导节点」区域 — 管理默认 + 自定义引导节点
 */

import { useState, useCallback } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { Plus, Trash2, RotateCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useNetworkStore } from "@/stores/network-store";
import { toast } from "sonner";

/** 默认引导节点（与后端 BOOTSTRAP_NODES 对应，只读展示） */
const DEFAULT_BOOTSTRAP_NODES = [
  "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
  "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
];

/** 简单的 Multiaddr 格式校验：必须包含 /p2p/ 且以 / 开头 */
function isValidMultiaddr(addr: string): boolean {
  return addr.startsWith("/") && addr.includes("/p2p/");
}

/** 截断 Multiaddr 用于显示 */
function truncateAddr(addr: string): string {
  if (addr.length <= 60) return addr;
  // 保留协议头和末尾 peer id
  const p2pIdx = addr.indexOf("/p2p/");
  if (p2pIdx === -1) return `${addr.slice(0, 30)}...${addr.slice(-20)}`;
  const prefix = addr.slice(0, Math.min(p2pIdx, 30));
  const peerId = addr.slice(p2pIdx + 5);
  const shortPeerId = peerId.length > 12
    ? `${peerId.slice(0, 6)}...${peerId.slice(-6)}`
    : peerId;
  return `${prefix}/p2p/${shortPeerId}`;
}

export function BootstrapNodesSection() {
  const { t } = useLingui();
  const customBootstrapNodes = usePreferencesStore((s) => s.customBootstrapNodes);
  const addBootstrapNode = usePreferencesStore((s) => s.addBootstrapNode);
  const removeBootstrapNode = usePreferencesStore((s) => s.removeBootstrapNode);
  const nodeStatus = useNetworkStore((s) => s.status);

  const [inputValue, setInputValue] = useState("");
  const [showInput, setShowInput] = useState(false);
  const [needsRestart, setNeedsRestart] = useState(false);
  const [restarting, setRestarting] = useState(false);

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
      DEFAULT_BOOTSTRAP_NODES.includes(addr)
    ) {
      toast.error(t(msg`该节点已存在`));
      return;
    }

    addBootstrapNode(addr);
    setInputValue("");
    setShowInput(false);

    if (nodeStatus === "running") {
      setNeedsRestart(true);
    }
  }

  function handleRemove(addr: string) {
    removeBootstrapNode(addr);
    if (nodeStatus === "running") {
      setNeedsRestart(true);
    }
  }

  const handleRestart = useCallback(async () => {
    setRestarting(true);
    try {
      const { stopNetwork, startNetwork } = useNetworkStore.getState();
      await stopNetwork();
      const ok = await startNetwork();
      if (!ok) {
        // startNetwork 失败时内部已 toast 原因；保持 needsRestart 供用户重试，
        // 不显示成功提示（避免把启动失败掩盖成"已重启"）。
        setNeedsRestart(true);
        return;
      }
      setNeedsRestart(false);
      toast.success(t(msg`节点已重启`));
    } catch {
      toast.error(t(msg`重启节点失败`));
    } finally {
      setRestarting(false);
    }
  }, [t]);

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold text-foreground">
        <Trans>高级引导节点</Trans>
      </h2>
      <div className="glass-card rounded-lg">
        {/* 默认节点 */}
        {DEFAULT_BOOTSTRAP_NODES.map((addr) => (
          <div
            key={addr}
            className="flex items-center justify-between border-b border-border p-4 last:border-b-0"
          >
            <div className="flex min-w-0 flex-1 items-center gap-2">
              <span className="truncate font-mono text-xs text-muted-foreground">
                {truncateAddr(addr)}
              </span>
              <Badge variant="secondary" className="shrink-0 text-[10px]">
                <Trans>默认</Trans>
              </Badge>
            </div>
          </div>
        ))}

        {/* 自定义节点 */}
        {customBootstrapNodes.map((addr) => (
          <div
            key={addr}
            className="flex items-center justify-between border-b border-border p-4 last:border-b-0"
          >
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">
              {truncateAddr(addr)}
            </span>
            <Button
              variant="ghost"
              size="icon"
              className="size-8 shrink-0 text-muted-foreground hover:text-destructive"
              onClick={() => handleRemove(addr)}
            >
              <Trash2 className="size-4" />
            </Button>
          </div>
        ))}

        {/* 添加输入框 */}
        {showInput ? (
          <div className="flex items-center gap-2 border-t border-border p-4">
            <Input
              value={inputValue}
              onChange={(e) => setInputValue(e.target.value)}
              placeholder={t(msg`输入 Multiaddr 地址，如 /ip4/.../p2p/...`)}
              className="flex-1 font-mono text-xs"
              onKeyDown={(e) => {
                if (e.key === "Enter") handleAdd();
                if (e.key === "Escape") {
                  setShowInput(false);
                  setInputValue("");
                }
              }}
              autoFocus
            />
            <Button size="sm" onClick={handleAdd}>
              <Trans>添加</Trans>
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setShowInput(false);
                setInputValue("");
              }}
            >
              <Trans>取消</Trans>
            </Button>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setShowInput(true)}
            className="flex w-full items-center gap-2 border-t border-border p-4 text-sm text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            <Plus className="size-4" />
            <Trans>添加自定义引导节点</Trans>
          </button>
        )}
      </div>

      {/* 需要重启提示 */}
      {needsRestart && nodeStatus === "running" && (
        <div className="flex items-center justify-between rounded-lg border border-amber-200 bg-amber-50 p-3 dark:border-amber-900 dark:bg-amber-950">
          <span className="text-xs text-amber-800 dark:text-amber-200">
            <Trans>引导节点已变更，需重启节点生效</Trans>
          </span>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            onClick={handleRestart}
            disabled={restarting}
          >
            <RotateCw className={`mr-1 size-3 ${restarting ? "animate-spin" : ""}`} />
            {restarting ? <Trans>重启中...</Trans> : <Trans>重启节点</Trans>}
          </Button>
        </div>
      )}
    </section>
  );
}
