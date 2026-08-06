"use client";

// 连接徽标（Device Card Contract 的 slot 6）+ 点开后的链路详情。
//
// 徽标本身回答「怎么连的」一句话；详情回答「凭什么这么说」——走的哪条地址、哪种传输、
// 经不经中继、经的是谁。后者对普通用户是噪音，所以收在 Popover 里，徽标看起来仍是一枚徽标。
// 桌面端 `src/routes/_app/devices/-components/connection-badge.tsx` 是同一形态的另一份实现。
//
// `connection` 与 `connectionDetails` 由内核同一次快照产出，不会互相矛盾
// （见 `crates/core/src/device_manager.rs` 的 `ConnectionSnapshot`）。

import { Trans, useLingui } from "@lingui/react/macro";
import { Check, Copy, TriangleAlert } from "lucide-react";
import { formatLatency, transportLabel } from "@swarmdrop/shared-view";
import { Badge } from "@/components/ui/badge";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/cn";
import { CONNECTION_META } from "../_lib/device-presentation";
import { useCopyToClipboard } from "../_lib/use-copy";
import type { Device } from "../_lib/view-types";

export function ConnectionBadge({ device }: { device: Device }) {
  const { t } = useLingui();

  const isOnline = device.status === "online";
  const meta = isOnline && device.connection ? CONNECTION_META[device.connection] : null;
  if (!meta) return null;

  const details = device.connectionDetails;
  const latency = formatLatency(device.latency);
  const transport = transportLabel(details?.transport);

  const badge = (
    <Badge
      variant="secondary"
      // `group-hover:border-current/30` 是这枚徽标可点时的 hover 反馈，只有被下面那个
      // `group` 按钮包起来时才生效（详情缺席时它是静态徽标，不该有任何 hover 表现）。
      //
      // **用描边而不是换底色**：侧栏那枚节点状态 pill 的 hover 是 `bg-accent`，但这里的底色
      // 是语义色（局域网绿 / 打洞蓝 / 中继琥珀，见 `CONNECTION_META`），盖成中性 accent 等于
      // 在 hover 的一瞬间把「怎么连的」这条信息抹掉。`current` 取的是徽标自己的文字色，
      // 于是三种连接各自得到自己那个颜色的描边——同一种交互语言（hover 有可见变化），
      // 各自说自己的话。
      className={cn("gap-1 border-transparent transition-colors group-hover:border-current/30", meta.className)}
    >
      <meta.Icon className="size-3" aria-hidden />
      <ConnectionLabel connection={device.connection} />
      {transport && <span className="opacity-70">{transport}</span>}
      {latency && <span className="font-mono tabular-nums">{latency}</span>}
    </Badge>
  );

  // 详情缺席时（内核还没报告过连接地址）徽标就是一枚静态徽标——摆一个点开是空的
  // Popover 比不给这个入口更糟。
  if (!details) return badge;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          // `focus-ring` 是全站统一的焦点表现（`global.css`，与桌面 `src/index.css` 同形），
          // 换掉此前手写的 `outline-none` + `focus-visible:ring-2`——手写那份少了 offset，
          // 描边贴着徽标边缘，在语义色底上几乎看不出来。
          className="focus-ring group rounded-full"
          aria-label={t`查看链路详情`}
          data-testid="connection-badge"
        >
          {badge}
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[min(20rem,calc(100vw-2rem))] space-y-3">
        <div className="space-y-0.5">
          <p className="text-sm font-medium">
            <Trans>链路详情</Trans>
          </p>
          <p className="text-xs text-muted-foreground">
            <Trans>排查连接问题时把这些贴进 issue</Trans>
          </p>
        </div>

        {device.lanUpgradeFailed && (
          // 只在「还挂着中继」时才有意义——升级成了 path 就不是 relay 了。
          // 这一句把两种在徽标上完全同形的状态分开：对端本来就在外网 vs
          // 对端就在同一网段却连不上。后者是可行动的，前者不是。
          <div className="flex gap-2 rounded-lg bg-amber-500/10 px-3 py-2.5 text-xs text-amber-700 dark:text-amber-300">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden />
            <div className="space-y-1">
              <p className="font-medium">
                <Trans>对方就在同一网段，但直连没建起来</Trans>
              </p>
              <p className="opacity-90">
                <Trans>
                  浏览器可能需要你允许「本地网络访问」；也可能是对方的防火墙拦了入站连接。
                  文件仍会经中继送达，只是更慢。
                </Trans>
              </p>
            </div>
          </div>
        )}

        <dl className="space-y-2 text-xs">
          <DetailRow label={t`传输`}>
            {transport ?? (
              <span className="text-muted-foreground">
                <Trans>未知</Trans>
              </span>
            )}
          </DetailRow>
          {details.relay && (
            <DetailRow label={t`中继节点`}>
              <span className="font-mono break-all">{details.relay}</span>
            </DetailRow>
          )}
          <DetailRow label={t`远端地址`}>
            <span className="font-mono break-all">{details.remoteAddr}</span>
          </DetailRow>
        </dl>

        {/* key 绑地址、且必须挂在**持有复制态的那个组件**上：链路升级（relay → LAN）
            后地址会换，而按钮上还挂着上一条的「已复制」，用户照着粘出去的是一条已经
            不在用的地址。key 挂在按钮 DOM 上没用——state 在父组件里，换代重置不到它。 */}
        <CopyAddressButton key={details.remoteAddr} address={details.remoteAddr} />
      </PopoverContent>
    </Popover>
  );
}

function CopyAddressButton({ address }: { address: string }) {
  const { state, copy } = useCopyToClipboard();
  return (
    <button
      type="button"
      onClick={() => void copy(address)}
      className="flex w-full items-center justify-center gap-1.5 rounded-lg border border-border py-1.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    >
      {state === "copied" ? (
        <Check className="size-3" aria-hidden />
      ) : (
        <Copy className="size-3" aria-hidden />
      )}
      {state === "copied" ? (
        <Trans>已复制</Trans>
      ) : state === "failed" ? (
        <Trans>复制失败，请手动选中</Trans>
      ) : (
        <Trans>复制远端地址</Trans>
      )}
    </button>
  );
}

function DetailRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[4.5rem_1fr] gap-2">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-foreground">{children}</dd>
    </div>
  );
}

function ConnectionLabel({ connection }: { connection: Device["connection"] }) {
  switch (connection) {
    case "lan":
      return <Trans>局域网</Trans>;
    case "dcutr":
      return <Trans>打洞</Trans>;
    case "relay":
      return <Trans>中继</Trans>;
    default:
      return null;
  }
}
