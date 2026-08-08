/**
 * StopNodeSheet
 * 停止节点确认弹窗（移动端 Bottom Sheet / 桌面端 Dialog）
 */

import { hostname, platform, type as osType } from "@tauri-apps/plugin-os";
import { useEffect, useState, useSyncExternalStore } from "react";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useShallow } from "zustand/shallow";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { cn } from "@/lib/utils";
import { formatUptime } from "@/lib/format-uptime";
import { useActiveTransferCount } from "@/hooks/use-active-transfer-count";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { LanHelperAddress } from "@/components/network/lan-helper-address";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import type { NodeStatus } from "@/stores/network-store";
import type { BootstrapCandidateSource } from "@/lib/bindings";

interface StopNodeSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/// 候选来源 → 标签。
///
/// 用 `Record` 而不是 switch / 三元链：类型上要求键齐全，`BootstrapCandidateSource`
/// 加第四个来源时这里**编译期**就红。此前它写成两级三元 + null 兜底，`learned`
/// （运行时经 identify 学到的中继，LAN Helper 引荐公网中继时的常态）掉进兜底，
/// 于是 relay 明明 Active、界面却把它渲染成「等待中」。
const relaySourceLabels: Record<BootstrapCandidateSource, MessageDescriptor> = {
  hostConfigured: msg`配置节点`,
  mdnsLanHelper: msg`局域网协助`,
  learned: msg`公网`,
};

const statusConfig: Record<
  NodeStatus,
  { label: MessageDescriptor; dotColor: string; className: string }
> = {
  stopped: {
    label: msg`未启动`,
    dotColor: "bg-muted-foreground",
    className: "bg-muted text-muted-foreground border-transparent",
  },
  starting: {
    label: msg`启动中`,
    dotColor: "bg-warning animate-pulse",
    className: "bg-warning/15 text-warning-ink border-transparent",
  },
  running: {
    label: msg`运行中`,
    dotColor: "bg-success",
    className: "bg-success/15 text-success-ink border-transparent",
  },
  error: {
    label: msg`错误`,
    dotColor: "bg-destructive",
    className: "bg-destructive/15 text-destructive-ink border-transparent",
  },
};

export function StopNodeSheet({ open, onOpenChange }: StopNodeSheetProps) {
  const { stopNetwork, status, networkStatus, startedAt } =
    useNetworkStore(
      useShallow((s) => ({
        stopNetwork: s.stopNetwork,
        status: s.status,
        networkStatus: s.networkStatus,
        startedAt: s.startedAt,
      })),
    );

  const listenAddrs = networkStatus?.listenAddrs ?? [];
  const connectedCount = networkStatus?.connectedPeers ?? 0;
  const discoveredCount = networkStatus?.discoveredPeers ?? 0;
  const natStatus = networkStatus?.natStatus ?? "unknown";
  const relayReady = networkStatus?.relayReady ?? false;
  const publicReachable = networkStatus?.publicReachable ?? false;
  const publicAddr = networkStatus?.publicAddr ?? null;
  const relayPeers = networkStatus?.relayPeers ?? [];
  const bootstrapConnected = networkStatus?.bootstrapConnected ?? false;
  const lanHelperCount = networkStatus?.lanHelperCount ?? 0;
  const bootstrapCandidateCount = networkStatus?.bootstrapCandidateCount ?? 0;
  const localLanHelperRunning = networkStatus?.localLanHelperRunning ?? false;
  const relaySource = networkStatus?.relaySource ?? null;

  const deviceId = useSecretStore((s) => s.deviceId);
  const deviceName = usePreferencesStore((s) => s.deviceName);

  const [systemHostname, setSystemHostname] = useState("");
  useEffect(() => {
    hostname().then((name) => setSystemHostname(name ?? ""));
  }, []);

  const currentPlatform = platform();
  const currentOsType = osType();
  const DeviceIcon = getDeviceIcon(currentOsType);

  const handleStop = async () => {
    await stopNetwork();
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto p-0! sm:max-w-[min(90vw,32rem)]">
        <StopNodeContent
          onStop={handleStop}
          onCancel={() => onOpenChange(false)}
          status={status}
          listenAddrs={listenAddrs}
          connectedCount={connectedCount}
          discoveredCount={discoveredCount}
          startedAt={startedAt}
          peerId={deviceId}
          deviceName={deviceName}
          systemHostname={systemHostname}
          platformName={currentPlatform}
          DeviceIcon={DeviceIcon}
          natStatus={natStatus}
          relayReady={relayReady}
          publicReachable={publicReachable}
          publicAddr={publicAddr}
          relayPeers={relayPeers}
          bootstrapConnected={bootstrapConnected}
          lanHelperCount={lanHelperCount}
          bootstrapCandidateCount={bootstrapCandidateCount}
          localLanHelperRunning={localLanHelperRunning}
          relaySource={relaySource}
        />
      </DialogContent>
    </Dialog>
  );
}

function StopNodeContent({
  onStop,
  onCancel,
  status,
  listenAddrs,
  connectedCount,
  discoveredCount,
  startedAt,
  peerId,
  deviceName,
  systemHostname,
  platformName,
  DeviceIcon,
  natStatus,
  relayReady,
  publicReachable,
  publicAddr,
  relayPeers,
  bootstrapConnected,
  lanHelperCount,
  bootstrapCandidateCount,
  localLanHelperRunning,
  relaySource,
}: {
  onStop: () => void;
  onCancel: () => void;
  status: NodeStatus;
  listenAddrs: string[];
  connectedCount: number;
  discoveredCount: number;
  startedAt: number | null;
  peerId: string | null;
  deviceName: string;
  systemHostname: string;
  platformName: string;
  DeviceIcon: React.ComponentType<{ className?: string }>;
  natStatus: string;
  relayReady: boolean;
  publicReachable: boolean;
  publicAddr: string | null;
  relayPeers: string[];
  bootstrapConnected: boolean;
  lanHelperCount: number;
  bootstrapCandidateCount: number;
  localLanHelperRunning: boolean;
  relaySource: BootstrapCandidateSource | null;
}) {
  const { t } = useLingui();
  const config = statusConfig[status];
  // 停止节点会断开全部连接，在途传输当场中断——这是这个弹窗唯一会造成数据损失的后果，
  // 必须说出来。不走 props：它与调用方传下来的那批网络快照不同源，也不该由调用方去查。
  const activeTransferCount = useActiveTransferCount();

  const windowHeight = useSyncExternalStore(
    (cb) => {
      window.addEventListener("resize", cb);
      return () => window.removeEventListener("resize", cb);
    },
    () => window.innerHeight,
  );
  const showExtra = windowHeight >= 700;

  const truncatedPeerId = peerId
    ? `${peerId.slice(0, 4)}...${peerId.slice(-5)}`
    : "—";

  const uptimeText = startedAt ? formatUptime(startedAt) : "—";
  const displayName = deviceName || systemHostname || "SwarmDrop";
  const avatarInitials = displayName.slice(0, 2).toUpperCase();

  const platformLabel: Record<string, string> = {
    windows: "Windows",
    macos: "macOS",
    linux: "Linux",
    android: "Android",
    ios: "iOS",
  };
  const relaySourceLabel = relaySource ? t(relaySourceLabels[relaySource]) : null;

  return (
    <div className="flex flex-col gap-4 p-6">
      <DialogHeader className="items-center text-center">
        {/* 设备身份卡片 */}
        <div className="relative">
          <div className="flex size-16 items-center justify-center rounded-2xl bg-primary/10 dark:bg-primary/12">
            <span className="text-xl font-bold tracking-tight text-brand">
              {avatarInitials}
            </span>
          </div>
          <div className="absolute -bottom-1 -right-1 flex size-6 items-center justify-center rounded-lg border border-border bg-background shadow-sm">
            <DeviceIcon className="size-3.5 text-muted-foreground" />
          </div>
        </div>
        <div>
          <DialogTitle>{displayName}</DialogTitle>
          <DialogDescription>
            {platformLabel[platformName] ?? platformName}
          </DialogDescription>
        </div>
      </DialogHeader>

      <div className="flex flex-col gap-3">
        {/* 统计数据 — 高度不足时隐藏 */}
        {showExtra && (
          <div className="grid grid-cols-2 gap-3">
            <div className="flex flex-col items-center gap-1 rounded-xl border border-border py-3">
              <span className="text-2xl font-bold text-foreground">
                {connectedCount}
              </span>
              <span className="text-xs text-muted-foreground">
                <Trans>已连接节点</Trans>
              </span>
            </div>
            <div className="flex flex-col items-center gap-1 rounded-xl border border-border py-3">
              <span className="text-2xl font-bold text-foreground">
                {discoveredCount}
              </span>
              <span className="text-xs text-muted-foreground">
                <Trans>已发现节点</Trans>
              </span>
            </div>
          </div>
        )}

        {/* 节点信息卡片 */}
        <div className="overflow-hidden rounded-xl border border-border">
          {/* 状态 */}
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-sm text-muted-foreground">
              <Trans>节点状态</Trans>
            </span>
            <Badge variant="outline" className={cn("gap-1.5", config.className)}>
              <span className={cn("size-2 rounded-full", config.dotColor)} />
              {t(config.label)}
            </Badge>
          </div>
          {/* Peer ID */}
          <div className="flex items-center justify-between border-t border-border px-4 py-3">
            <span className="text-sm text-muted-foreground">Peer ID</span>
            <code className="font-mono text-sm text-foreground">
              {truncatedPeerId}
            </code>
          </div>
          {/* 运行时长 */}
          <div className="flex items-center justify-between border-t border-border px-4 py-3">
            <span className="text-sm text-muted-foreground">
              <Trans>运行时长</Trans>
            </span>
            <span className="text-sm font-medium text-foreground">
              {uptimeText}
            </span>
          </div>
          {/* NAT 状态 */}
          <div className="flex items-center justify-between border-t border-border px-4 py-3">
            <span className="text-sm text-muted-foreground">
              <Trans>NAT 状态</Trans>
            </span>
            <Badge
              variant="outline"
              className={cn(
                "border-transparent text-xs",
                natStatus === "public"
                  ? "bg-primary/10 text-brand dark:bg-primary/15"
                  : "bg-muted text-muted-foreground",
              )}
            >
              {natStatus === "public" ? t`映射成功` : t`未知`}
            </Badge>
          </div>
          {/* 公网可达性 — 区分"设备离线"与"跨网不可直达" */}
          <div className="flex items-center justify-between border-t border-border px-4 py-3">
            <span className="text-sm text-muted-foreground">
              <Trans>公网可达</Trans>
            </span>
            <Badge
              variant="outline"
              className={cn(
                "border-transparent text-xs",
                publicReachable
                  ? "bg-success/15 text-success-ink"
                  : "bg-muted text-muted-foreground",
              )}
            >
              {publicReachable ? t`可达` : t`仅局域网`}
            </Badge>
          </div>
          {/* 中继状态 — 高度不足时隐藏 */}
          {showExtra && (
            <div className="flex items-center justify-between border-t border-border px-4 py-3">
              <span className="text-sm text-muted-foreground">
                <Trans>中继节点</Trans>
              </span>
              <div className="flex items-center gap-2">
                {relayPeers.length > 0 && (
                  <span className="text-xs tabular-nums text-muted-foreground">
                    {relayPeers.length}
                  </span>
                )}
                <Badge
                  variant="outline"
                  className={cn(
                    "border-transparent text-xs",
                    relayReady
                      ? "bg-success/15 text-success-ink"
                      : "bg-muted text-muted-foreground",
                  )}
                >
                  {relayReady ? t`已就绪` : t`未连接`}
                </Badge>
              </div>
            </div>
          )}
          {/* 引导节点 — 高度不足时隐藏 */}
          {showExtra && (
            <div className="flex items-center justify-between border-t border-border px-4 py-3">
              <span className="text-sm text-muted-foreground">
                <Trans>引导节点</Trans>
              </span>
              <Badge
                variant="outline"
                className={cn(
                  "border-transparent text-xs",
                  bootstrapConnected
                    ? "bg-success/15 text-success-ink"
                    : "bg-muted text-muted-foreground",
                )}
              >
                {bootstrapConnected ? t`已连接` : t`未连接`}
              </Badge>
            </div>
          )}
          {showExtra && (
            <div className="flex items-center justify-between border-t border-border px-4 py-3">
              <span className="text-sm text-muted-foreground">
                <Trans>局域网协助</Trans>
              </span>
              <div className="flex items-center gap-2">
                <span className="text-xs tabular-nums text-muted-foreground">
                  {lanHelperCount}
                </span>
                <Badge
                  variant="outline"
                  className={cn(
                    "border-transparent text-xs",
                    localLanHelperRunning
                      ? "bg-success/15 text-success-ink"
                      : "bg-muted text-muted-foreground",
                  )}
                >
                  {localLanHelperRunning ? t`本机协助中` : t`未提供`}
                </Badge>
              </div>
            </div>
          )}
          {showExtra && (
            <div className="flex items-center justify-between border-t border-border px-4 py-3">
              <span className="text-sm text-muted-foreground">
                <Trans>候选来源</Trans>
              </span>
              <div className="flex items-center gap-2">
                <span className="text-xs tabular-nums text-muted-foreground">
                  {bootstrapCandidateCount}
                </span>
                <Badge variant="outline" className="border-transparent bg-muted text-xs text-muted-foreground">
                  {relaySourceLabel ?? t`等待中`}
                </Badge>
              </div>
            </div>
          )}
          {/* 公网地址 — 高度不足时隐藏 */}
          {showExtra && publicAddr && (
            <div className="flex items-center justify-between border-t border-border px-4 py-3">
              <span className="text-sm text-muted-foreground">
                <Trans>公网地址</Trans>
              </span>
              <code className="max-w-55 truncate font-mono text-xs text-foreground">
                {publicAddr}
              </code>
            </div>
          )}
        </div>

        {/* 局域网协助地址 — 供浏览器端快速连接（仅本机开启协助时展示） */}
        <LanHelperAddress />

        {/* 监听地址（折叠）— 高度不足时隐藏 */}
        {showExtra && listenAddrs.length > 0 && (
          <details className="group rounded-xl border border-border">
            <summary className="flex cursor-pointer items-center justify-between px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground">
              <Trans>监听地址</Trans>
              <span className="text-xs tabular-nums">
                {listenAddrs.length}
              </span>
            </summary>
            <div className="max-h-32 overflow-y-auto border-t border-border px-4 py-2.5">
              <div className="flex flex-col gap-1">
                {listenAddrs.map((addr, i) => (
                  <code
                    key={i}
                    className="break-all font-mono text-[11px] leading-relaxed text-muted-foreground"
                  >
                    {addr}
                  </code>
                ))}
              </div>
            </div>
          </details>
        )}
      </div>

      {/* 警告 + 按钮 */}
      <DialogFooter className="flex flex-col gap-3">
        <p className="text-center text-xs text-destructive-ink">
          <Trans>停止后将断开所有连接，其他设备将无法发现你</Trans>
          {/* 「断开所有连接」说的是可达性，用户读不出「我正在传的文件会没」。
              在途会话数必须单独说一句——这是唯一会造成数据损失的后果。 */}
          {activeTransferCount > 0 && (
            <>
              {" "}
              <Trans>
                正在进行的 {activeTransferCount} 个传输会被中断。
              </Trans>
            </>
          )}
        </p>
        <div className="flex gap-2">
          <Button variant="outline" onClick={onCancel} className="flex-1">
            <Trans>取消</Trans>
          </Button>
          <Button variant="destructive" onClick={onStop} className="flex-1">
            <Trans>停止节点</Trans>
          </Button>
        </div>
      </DialogFooter>
    </div>
  );
}
