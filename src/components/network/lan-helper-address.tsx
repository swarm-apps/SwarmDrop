/**
 * LanHelperAddress
 *
 * 展示本机作为「局域网协助节点」的可拨号地址（ws multiaddr + `/p2p/<peerId>`），
 * 供浏览器端快速连接 / reserve。仅在本机开启协助能力且存在 ws 监听地址时展示。
 *
 * 数据源：`networkStatus.lanHelperAdvertisedAddrs`（后端仅在 `provide_lan_helper`
 * 开启时填充为私网监听地址）。这些是裸监听地址、不含 `/p2p/` 段，故需拼上本机 peerId。
 */

import { useMemo } from "react";
import { Copy, Wifi } from "lucide-react";
import { toast } from "sonner";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { useNetworkStore } from "@/stores/network-store";
import { copyText } from "@/lib/clipboard";
import { cn } from "@/lib/utils";

/**
 * 从 `networkStatus` 派生浏览器可拨的协助地址：筛出 ws 地址、拼上 `/p2p/<peerId>`。
 *
 * 注意：selector 只取稳定的 `networkStatus` 引用，派生在 `useMemo` 内做——
 * 直接在 selector 里 `filter/map` 会每次返回新数组引用导致无限 re-render。
 */
export function useLanHelperAddresses(): string[] {
  const networkStatus = useNetworkStore((s) => s.networkStatus);
  return useMemo(() => {
    const peerId = networkStatus?.peerId;
    const addrs = networkStatus?.lanHelperAdvertisedAddrs ?? [];
    if (!peerId) return [];
    return addrs
      .filter((addr) => addr.includes("/ws"))
      .map((addr) => `${addr}/p2p/${peerId}`);
  }, [networkStatus?.peerId, networkStatus?.lanHelperAdvertisedAddrs]);
}

export function LanHelperAddress({ className }: { className?: string }) {
  const { t } = useLingui();
  const addresses = useLanHelperAddresses();

  if (addresses.length === 0) return null;

  const handleCopy = async (addr: string) => {
    try {
      await copyText(addr);
      toast.success(t`协助地址已复制`);
    } catch {
      toast.error(t`复制失败`);
    }
  };

  return (
    <div className={cn("overflow-hidden rounded-xl border border-border", className)}>
      <div className="flex items-center gap-2 border-b border-border px-4 py-2.5">
        <Wifi className="size-3.5 text-brand" />
        <span className="text-sm font-medium text-foreground">
          <Trans>局域网协助地址</Trans>
        </span>
      </div>
      <div className="flex flex-col divide-y divide-border">
        {addresses.map((addr) => (
          <button
            key={addr}
            type="button"
            onClick={() => handleCopy(addr)}
            title={t`点击复制`}
            className="group flex items-center gap-2 px-4 py-2.5 text-left transition-colors hover:bg-muted/50"
          >
            <code className="flex-1 break-all font-mono text-[11px] leading-relaxed text-muted-foreground">
              {addr}
            </code>
            <Copy className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
          </button>
        ))}
      </div>
      <p className="border-t border-border px-4 py-2 text-xs text-muted-foreground">
        <Trans>浏览器端可用此地址快速连接本机</Trans>
      </p>
    </div>
  );
}
