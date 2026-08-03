import { type ComponentType, useEffect, useRef, useState } from "react";
import { Check, Copy, RadioTower, TriangleAlert, Wifi, Zap } from "lucide-react";
import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import { transportLabel } from "@swarmdrop/shared-view";
import { toast } from "sonner";
import type { ConnectionType, Device } from "@/lib/bindings";
import { copyText } from "@/lib/clipboard";
import { formatLatency } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";

/**
 * 连接徽标（Device Card Contract 的 slot 6）+ 点开后的链路详情。
 *
 * 徽标本身回答「怎么连的」一句话；详情回答「凭什么这么说」——走的哪条地址、
 * 哪种传输、经不经中继、经的是谁。后者对普通用户是噪音，所以收在 Popover 里，
 * 徽标看起来仍是一枚徽标。
 *
 * `connection` 与 `connectionDetails` 由内核同一次快照产出，不会互相矛盾
 * （见 `crates/core/src/device_manager.rs` 的 `ConnectionSnapshot`）。
 */
const connectionConfig: Record<
  ConnectionType,
  {
    icon: ComponentType<{ className?: string }>;
    label: MessageDescriptor;
    bgColor: string;
    textColor: string;
  }
> = {
  lan: {
    icon: Wifi,
    label: msg`局域网`,
    bgColor: "bg-green-100",
    textColor: "text-green-600",
  },
  dcutr: {
    icon: Zap,
    label: msg`打洞`,
    bgColor: "bg-sky-100",
    textColor: "text-sky-600",
  },
  relay: {
    icon: RadioTower,
    label: msg`中继`,
    bgColor: "bg-amber-100",
    textColor: "text-amber-600",
  },
};

export function ConnectionBadge({ device }: { device: Device }) {
  const { t } = useLingui();
  const config = device.connection ? connectionConfig[device.connection] : null;
  const details = device.connectionDetails;

  if (!config) return null;

  const latency = device.latency != null ? formatLatency(device.latency) : null;
  const transport = transportLabel(details?.transport);

  const badge = (
    <span
      className={cn(
        "flex items-center gap-1 rounded-full px-2.5 py-1 ring-1 ring-black/[0.03]",
        config.bgColor,
      )}
    >
      <config.icon className={cn("size-2.5", config.textColor)} />
      <span className={cn("text-[10px] font-medium", config.textColor)}>
        {t(config.label)}
      </span>
      {transport && (
        <span className={cn("text-[10px] font-medium opacity-70", config.textColor)}>
          {transport}
        </span>
      )}
      {latency && (
        <span className={cn("text-[10px] font-medium", config.textColor)}>
          {latency}
        </span>
      )}
    </span>
  );

  // 详情缺席时（离线、或内核还没报告过连接地址）徽标就是一枚静态徽标——
  // 摆一个点开是空的 Popover 比不给这个入口更糟。
  if (!details) return badge;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          // 整张卡片是「发送」按钮，徽标是嵌套控件——不拦住冒泡点详情会触发发送
          onClick={(e) => e.stopPropagation()}
          className="cursor-pointer rounded-full outline-none transition-opacity hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring"
          aria-label={t`查看链路详情`}
          data-testid="connection-badge"
        >
          {badge}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-80 space-y-3"
        onClick={(e) => e.stopPropagation()}
      >
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
          //
          // 桌面端不提浏览器权限（那是 Web 端的成因），只说防火墙。
          <div className="flex gap-2 rounded-lg bg-amber-500/10 px-3 py-2.5 text-xs text-amber-700 dark:text-amber-300">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
            <div className="space-y-1">
              <p className="font-medium">
                <Trans>对方就在同一网段，但直连没建起来</Trans>
              </p>
              <p className="opacity-90">
                <Trans>
                  通常是某一端的防火墙拦了入站连接。文件仍会经中继送达，只是更慢。
                </Trans>
              </p>
            </div>
          </div>
        )}

        <dl className="space-y-2 text-xs">
          <DetailRow label={t`传输`}>
            {/* 入站中继连接的 send_back_addr 只有 /p2p/<src> 一段，地址里确实读不出
                传输——照实说「未知」，不要编一个默认值 */}
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
  const { t } = useLingui();
  const [copied, setCopied] = useState(false);
  const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  // 确认还挂着时 Popover 被关掉 / 组件卸载，定时器要跟着走
  useEffect(() => () => clearTimeout(resetTimer.current), []);

  const handleCopy = async () => {
    // 连点要顶掉上一个 timer，否则前一次的到点会把这一次的确认提前掐掉
    clearTimeout(resetTimer.current);
    try {
      await copyText(address);
      setCopied(true);
      resetTimer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
      // 桌面端有 toast 系统，失败走它——按钮上只留成功这一种瞬时态
      toast.error(t`复制失败`);
    }
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      className="flex w-full items-center justify-center gap-1.5 rounded-lg border border-border/60 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
    >
      {copied ? (
        <>
          <Check className="size-3" />
          <Trans>已复制</Trans>
        </>
      ) : (
        <>
          <Copy className="size-3" />
          <Trans>复制远端地址</Trans>
        </>
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
