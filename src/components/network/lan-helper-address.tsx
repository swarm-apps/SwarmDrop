/**
 * LanHelperAddress
 *
 * 展示本机作为「局域网协助节点」的可拨号地址，供浏览器端快速连接。
 * 仅在本机开启协助能力且存在浏览器可用监听地址时展示。
 *
 * 数据源：`networkStatus.lanHelperAdvertisedAddrs`（后端仅在 `provide_lan_helper`
 * 开启时填充为私网监听地址）。这些是裸监听地址、不含 `/p2p/` 段。
 *
 * **筛选与排序全在 `@swarmdrop/shared-view`**——桌面与移动此前各有一份逐行同构的
 * 白名单三元链，WebTransport 上线后两端都把它静默丢掉了。列表按吞吐排序，第一条最快。
 */

import { useMemo } from "react";
import { Copy, RadioTower } from "lucide-react";
import { toast } from "sonner";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { lanHelperAddresses, type LanHelperAddress as LanHelperAddressItem } from "@swarmdrop/shared-view";
import { useNetworkStore } from "@/stores/network-store";
import { copyText } from "@/lib/clipboard";
import { cn } from "@/lib/utils";

/**
 * 派生浏览器可拨的协助地址。
 *
 * selector 只取稳定的 `networkStatus` 引用，派生放 `useMemo`——直接在 selector 里
 * `filter/map` 会每次返回新数组引用导致无限 re-render。
 */
export function useLanHelperAddresses(): LanHelperAddressItem[] {
  const networkStatus = useNetworkStore((s) => s.networkStatus);
  return useMemo(
    () =>
      lanHelperAddresses(
        networkStatus?.lanHelperAdvertisedAddrs ?? [],
        networkStatus?.peerId ?? undefined,
      ),
    [networkStatus?.peerId, networkStatus?.lanHelperAdvertisedAddrs],
  );
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
    <section
      className={cn("glass-card overflow-hidden rounded-[20px]", className)}
      aria-label={t`局域网协助地址`}
    >
      <div className="flex items-start gap-3 border-b border-border/60 p-4">
        <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-brand/10">
          <RadioTower className="size-4 text-brand" />
        </div>
        <div className="min-w-0">
          <h3 className="text-sm font-medium text-foreground">
            <Trans>局域网协助地址</Trans>
          </h3>
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
            <Trans>复制后可供浏览器端快速连接本机。</Trans>
          </p>
        </div>
      </div>
      <div className="divide-y divide-border/60">
        {addresses.map(({ address, transport }) => (
          <button
            key={address}
            type="button"
            onClick={() => handleCopy(address)}
            title={t`点击复制`}
            className="group flex w-full items-center gap-3 p-4 text-left transition-colors hover:bg-muted/50"
          >
            <div className="min-w-0 flex-1">
              <span className="mb-1 inline-flex rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                {transport}
              </span>
              <code className="block break-all font-mono text-[11px] leading-relaxed text-muted-foreground">
                {address}
              </code>
            </div>
            <Copy className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
          </button>
        ))}
      </div>
    </section>
  );
}
