/**
 * Devices Page (Lazy)
 * 桌面端主屏 —— 设备发现、快速配对、已配对设备和活跃传输
 * 移动端已迁移到 SwarmDrop-RN,此处仅桌面端
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { DeviceCard } from "./-components/device-card";
import { TransferItem } from "../transfer/-transfer-item";
import type { Device } from "@/lib/bindings";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePairingStore } from "@/stores/pairing-store";
import { useTransferStore } from "@/stores/transfer-store";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { useCountdown } from "@/hooks/use-countdown";
import { commands } from "@/lib/bindings";
import { OfflineEmptyState } from "./-components/offline-empty-state";
import { StartNodeSheet } from "@/components/network/start-node-sheet";
import { StopNodeSheet } from "@/components/network/stop-node-sheet";
import { cn } from "@/lib/utils";
import { deviceDisplayName } from "@/lib/device-name";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { formatCountdown } from "@/lib/format";
import {
  DesktopDeviceFoundContent,
  useDeviceFoundState,
} from "@/routes/_app/pairing/-device-found-view";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSeparator,
  InputOTPSlot,
} from "@/components/ui/input-otp";
import {
  ArrowUpRight,
  Check,
  Clock,
  Copy,
  Keyboard,
  Link as LinkIcon,
  Loader2,
  MonitorSmartphone,
  Radio,
  RefreshCw,
  Send,
  Wifi,
} from "lucide-react";

export const Route = createLazyFileRoute("/_app/devices/")({
  component: DevicesPage,
});

type NearbyFilter = "all" | "unpaired" | "paired";

const nearbyFilterOptions: Array<{
  value: NearbyFilter;
  label: React.ReactNode;
}> = [
  { value: "all", label: <Trans>全部</Trans> },
  { value: "unpaired", label: <Trans>可配对</Trans> },
  { value: "paired", label: <Trans>已配对</Trans> },
];

function DevicesPage() {
  const navigate = useNavigate();

  const devices = useNetworkStore((s) => s.devices);
  const status = useNetworkStore((s) => s.status);
  const isOnline = status === "running" || status === "starting";
  const storedPairedDevices = useSecretStore((state) => state.pairedDevices);
  const directPairing = usePairingStore((state) => state.directPairing);
  const sessions = useTransferStore((s) => s.sessions);

  // directPairing 成功后自动跳转到设备页面(刷新列表)
  usePairingSuccess();

  // 节点控制弹窗状态
  const [startSheetOpen, setStartSheetOpen] = useState(false);
  const [stopSheetOpen, setStopSheetOpen] = useState(false);

  // 已配对设备:后端在线数据优先,离线回退到 secret-store
  const pairedPeerIds = useMemo(
    () => new Set(storedPairedDevices.map((device) => device.peerId)),
    [storedPairedDevices],
  );

  const normalizedDevices = useMemo<Device[]>(
    () =>
      devices.map((device) =>
        pairedPeerIds.has(device.peerId)
          ? {
              ...device,
              isPaired: true,
            }
          : device,
      ),
    [devices, pairedPeerIds],
  );

  const pairedDevices = useMemo<Device[]>(() => {
    const deviceMap = new Map(normalizedDevices.map((d) => [d.peerId, d]));
    return storedPairedDevices
      .map((stored) => {
        const backendDevice = deviceMap.get(stored.peerId);
        if (backendDevice) {
          return {
            ...backendDevice,
            isPaired: true,
          };
        }
        // 节点未运行或设备离线,用 secret-store 数据显示为离线
        return {
          peerId: stored.peerId,
          name: stored.name,
          hostname: stored.hostname,
          os: stored.os,
          platform: stored.platform,
          arch: stored.arch,
          status: "offline" as const,
          connection: null,
          latency: null,
          isPaired: true,
        };
      })
      .sort((a, b) => {
        if (a.status === b.status) {
          return deviceDisplayName(a).localeCompare(deviceDisplayName(b));
        }
        return a.status === "online" ? -1 : 1;
      });
  }, [storedPairedDevices, normalizedDevices]);

  const nearbyDevices = useMemo(
    () =>
      normalizedDevices
        .filter((device) => device.status === "online")
        .sort((a, b) => deviceDisplayName(a).localeCompare(deviceDisplayName(b))),
    [normalizedDevices],
  );

  const activeSessionIds = useMemo(
    () =>
      Object.values(sessions)
        .sort((a, b) => b.startedAt - a.startedAt)
        .map((session) => session.sessionId),
    [sessions],
  );

  const handleSend = (device: Device) => {
    navigate({ to: "/send", search: { peerId: device.peerId } });
  };

  const handleConnect = (device: Device) => {
    directPairing(device.peerId);
  };

  const handleUnpair = (device: Device) => {
    // 同时更新后端运行时状态(节点未运行时静默成功)
    commands.removePairedDevice(device.peerId);
    useSecretStore.getState().removePairedDevice(device.peerId);
  };

  return (
    <>
      <DesktopDevicesView
        isOnline={isOnline}
        nearbyDevices={nearbyDevices}
        pairedDevices={pairedDevices}
        activeSessionIds={activeSessionIds}
        onSend={handleSend}
        onConnect={handleConnect}
        onUnpair={handleUnpair}
        onStartClick={() => setStartSheetOpen(true)}
      />

      {/* 节点控制弹窗 */}
      <StartNodeSheet open={startSheetOpen} onOpenChange={setStartSheetOpen} />
      <StopNodeSheet open={stopSheetOpen} onOpenChange={setStopSheetOpen} />
    </>
  );
}

interface DesktopDevicesViewProps {
  isOnline: boolean;
  nearbyDevices: Device[];
  pairedDevices: Device[];
  activeSessionIds: string[];
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
  onUnpair: (device: Device) => void;
  onStartClick: () => void;
}

function DesktopDevicesView({
  isOnline,
  nearbyDevices,
  pairedDevices,
  activeSessionIds,
  onSend,
  onConnect,
  onUnpair,
  onStartClick,
}: DesktopDevicesViewProps) {
  // 桌面端主屏:设备发现 / 快速配对 / 已配对设备 / 活跃传输 —— 顶栏由全局 AppTopBar 承载
  return (
    <main className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      {isOnline ? (
        <div className="flex-1 overflow-auto bg-[radial-gradient(circle_at_18%_0%,rgba(37,99,235,0.08),transparent_30%),linear-gradient(180deg,rgba(248,250,252,0.92),rgba(255,255,255,1)_42%)] dark:bg-[radial-gradient(circle_at_18%_0%,rgba(59,130,246,0.12),transparent_30%),linear-gradient(180deg,rgba(24,24,27,0.98),rgba(9,9,11,1)_46%)]">
          <div className="mx-auto grid w-full max-w-[1220px] gap-5 px-5 py-5 min-[920px]:grid-cols-[minmax(0,1fr)_360px] lg:grid-cols-[minmax(0,1fr)_380px] lg:px-8 lg:py-7">
            <HomeOverview
              nearbyCount={nearbyDevices.length}
              pairedCount={pairedDevices.length}
              activeCount={activeSessionIds.length}
            />

            <div className="flex min-w-0 flex-col gap-5">
              <PairedDevicesSection
                devices={pairedDevices}
                onSend={onSend}
                onConnect={onConnect}
                onUnpair={onUnpair}
              />
              <ActiveTransfersSection sessionIds={activeSessionIds} />
            </div>

            <aside className="flex min-w-0 flex-col gap-5">
              <AddDeviceSection
                devices={nearbyDevices}
                onSend={onSend}
                onConnect={onConnect}
              />
            </aside>
          </div>
        </div>
      ) : (
        <OfflineEmptyState onStartClick={onStartClick} />
      )}
    </main>
  );
}

function HomeOverview({
  nearbyCount,
  pairedCount,
  activeCount,
}: {
  nearbyCount: number;
  pairedCount: number;
  activeCount: number;
}) {
  return (
    <section className="min-[920px]:col-span-2">
      <div className="overflow-hidden rounded-[24px] border border-black/[0.04] bg-white/70 p-1 shadow-[0_18px_60px_rgba(15,23,42,0.06)] ring-1 ring-white/80 dark:border-white/10 dark:bg-white/[0.035] dark:ring-white/10">
        <div className="flex flex-col gap-4 rounded-[20px] bg-white px-5 py-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.92)] dark:bg-zinc-950/80 dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.08)] sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-xs font-medium text-blue-600 dark:text-blue-400">
              <span className="flex size-7 items-center justify-center rounded-full bg-blue-50 dark:bg-blue-500/15">
                <MonitorSmartphone className="size-3.5" />
              </span>
              <Trans>设备中心</Trans>
            </div>
            <h1 className="mt-2 text-xl font-semibold tracking-tight text-foreground">
              <Trans>发现设备，配对，然后发送文件</Trans>
            </h1>
            <p className="mt-1 max-w-[58ch] text-sm leading-6 text-muted-foreground">
              <Trans>已配对设备优先展示，附近设备和配对入口收在右侧，打开应用就能开始操作。</Trans>
            </p>
          </div>

          <div className="grid grid-cols-3 gap-2 sm:min-w-[300px]">
            <OverviewStat label={<Trans>附近</Trans>} value={nearbyCount} />
            <OverviewStat label={<Trans>已配对</Trans>} value={pairedCount} />
            <OverviewStat label={<Trans>传输中</Trans>} value={activeCount} />
          </div>
        </div>
      </div>
    </section>
  );
}

function OverviewStat({
  label,
  value,
}: {
  label: React.ReactNode;
  value: number;
}) {
  return (
    <div className="rounded-[16px] bg-zinc-50 px-3 py-2.5 text-center ring-1 ring-black/[0.04] dark:bg-white/[0.04] dark:ring-white/10">
      <div className="font-mono text-lg font-semibold text-foreground">
        {value}
      </div>
      <div className="mt-0.5 text-[11px] text-muted-foreground">{label}</div>
    </div>
  );
}

function SectionHeader({
  title,
  count,
  icon: Icon,
  description,
}: {
  title: React.ReactNode;
  count?: number;
  icon?: React.ComponentType<{ className?: string }>;
  description?: React.ReactNode;
}) {
  return (
    <div className="flex min-w-0 items-start justify-between gap-3">
      <div className="flex min-w-0 gap-2.5">
        {Icon && (
          <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-zinc-100 text-muted-foreground dark:bg-white/[0.06]">
            <Icon className="size-3.5" />
          </span>
        )}
        <div className="min-w-0">
          <h2 className="truncate text-[15px] font-semibold tracking-tight text-foreground">
            {title}
          </h2>
          {description && (
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              {description}
            </p>
          )}
        </div>
      </div>
      {typeof count === "number" && (
        <span className="rounded-full bg-zinc-100 px-2.5 py-1 text-[11px] font-semibold text-muted-foreground ring-1 ring-black/[0.03] dark:bg-white/[0.06] dark:ring-white/10">
          {count}
        </span>
      )}
    </div>
  );
}

function SectionShell({
  children,
  className,
  innerClassName,
}: {
  children: React.ReactNode;
  className?: string;
  innerClassName?: string;
}) {
  return (
    <section
      className={cn(
        "rounded-[24px] border border-black/[0.04] bg-white/65 p-1 shadow-[0_16px_50px_rgba(15,23,42,0.055)] ring-1 ring-white/80 dark:border-white/10 dark:bg-white/[0.035] dark:ring-white/10",
        className,
      )}
    >
      <div
        className={cn(
          "flex min-h-full flex-col gap-4 rounded-[20px] bg-white p-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.92)] dark:bg-zinc-950/80 dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.08)]",
          innerClassName,
        )}
      >
        {children}
      </div>
    </section>
  );
}

function EmptyPanel({
  title,
  description,
  className,
}: {
  title: React.ReactNode;
  description: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "rounded-[18px] border border-dashed border-zinc-200 bg-zinc-50/70 px-4 py-5 dark:border-white/10 dark:bg-white/[0.035]",
        className,
      )}
    >
      <p className="text-sm font-medium text-foreground">{title}</p>
      <p className="mt-1 text-[13px] leading-5 text-muted-foreground">
        {description}
      </p>
    </div>
  );
}

function AddDeviceSection({
  devices,
  onSend,
  onConnect,
}: {
  devices: Device[];
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
}) {
  const ensureActiveCode = usePairingStore((state) => state.ensureActiveCode);
  const regenerateCode = usePairingStore((state) => state.regenerateCode);
  const codeInfo = usePairingStore((state) => state.activeCode);
  const errorMessage = usePairingStore((state) => state.codeError);
  const { remainingSeconds, isExpired } = useCountdown(
    codeInfo?.expiresAt ?? null,
  );
  const [copied, setCopied] = useState(false);
  const [inputOpen, setInputOpen] = useState(false);
  const [nearbyFilter, setNearbyFilter] = useState<NearbyFilter>("all");

  const filteredDevices = useMemo(() => {
    if (nearbyFilter === "paired") {
      return devices.filter((device) => device.isPaired);
    }
    if (nearbyFilter === "unpaired") {
      return devices.filter((device) => !device.isPaired);
    }
    return devices;
  }, [devices, nearbyFilter]);

  useEffect(() => {
    ensureActiveCode();
  }, [ensureActiveCode]);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1800);
    return () => clearTimeout(timer);
  }, [copied]);

  const handleCopy = useCallback(async () => {
    if (!codeInfo) return;
    try {
      await navigator.clipboard.writeText(codeInfo.code);
      setCopied(true);
    } catch {
      toast.error(t`复制失败，请手动复制配对码`);
    }
  }, [codeInfo]);

  const isLoading = codeInfo === null && errorMessage === null;

  return (
    <SectionShell innerClassName="gap-3.5">
      <SectionHeader
        title={<Trans>添加设备</Trans>}
        count={devices.length}
        icon={LinkIcon}
        description={<Trans>附近设备优先，已配对设备可直接发送。</Trans>}
      />

      <div className="rounded-[18px] bg-zinc-50/80 p-3 ring-1 ring-black/[0.04] dark:bg-white/[0.04] dark:ring-white/10">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <Radio className="size-3.5" />
            <Trans>附近设备</Trans>
          </div>
          <NearbyFilterControl
            value={nearbyFilter}
            onChange={setNearbyFilter}
          />
        </div>

        {filteredDevices.length === 0 ? (
          <div className="mt-2 rounded-[15px] border border-dashed border-zinc-200 bg-white/70 px-3 py-3 dark:border-white/10 dark:bg-zinc-950/35">
            <p className="text-sm font-medium text-foreground">
              <Trans>没有符合条件的附近设备</Trans>
            </p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              <Trans>切换过滤条件，或直接使用下方配对码连接。</Trans>
            </p>
          </div>
        ) : (
          <div className="mt-2 flex flex-col gap-2">
            {filteredDevices.map((device) => (
              <NearbyDeviceRow
                key={device.peerId}
                device={device}
                onSend={onSend}
                onConnect={onConnect}
              />
            ))}
          </div>
        )}
      </div>

      <div className="overflow-hidden rounded-[16px] bg-[linear-gradient(135deg,rgba(239,246,255,0.78),rgba(255,255,255,0.86))] p-2.5 ring-1 ring-blue-600/10 dark:bg-[linear-gradient(135deg,rgba(37,99,235,0.12),rgba(255,255,255,0.035))] dark:ring-blue-400/15">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <p className="text-[13px] font-medium text-foreground">
              <Trans>本机配对码</Trans>
            </p>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              <Trans>让另一台设备输入这组 6 位数字</Trans>
            </p>
          </div>
          {codeInfo && (
            <div className="flex shrink-0 items-center gap-1 rounded-full bg-white/75 px-2 py-0.5 text-[11px] text-muted-foreground ring-1 ring-black/[0.04] dark:bg-zinc-950/60 dark:ring-white/10">
              <Clock className="size-3" />
              {isExpired ? (
                <Trans>已过期</Trans>
              ) : (
                <span>{formatCountdown(remainingSeconds)}</span>
              )}
            </div>
          )}
        </div>

        <div className="mt-2.5 grid grid-cols-[minmax(0,1fr)_40px_40px] gap-1.5">
          <div className="flex h-10 min-w-0 items-center justify-center rounded-[12px] bg-white px-2.5 shadow-[inset_0_1px_0_rgba(255,255,255,0.9)] ring-1 ring-black/[0.04] dark:bg-zinc-950/70 dark:ring-white/10">
            {isLoading ? (
              <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
            ) : errorMessage ? (
              <span className="truncate text-[11px] text-destructive">
                {errorMessage}
              </span>
            ) : (
              <span className="font-mono text-[20px] font-semibold tracking-[0.16em] text-foreground">
                {codeInfo?.code}
              </span>
            )}
          </div>
          <button
            type="button"
            onClick={handleCopy}
            disabled={!codeInfo || isExpired}
            aria-label={copied ? t`已复制` : t`复制配对码`}
            title={copied ? t`已复制` : t`复制配对码`}
            className="flex size-10 items-center justify-center rounded-[12px] bg-white text-muted-foreground shadow-[inset_0_1px_0_rgba(255,255,255,0.9)] ring-1 ring-black/[0.05] transition-[background-color,color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-blue-50 hover:text-blue-600 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 dark:bg-zinc-950/70 dark:ring-white/10 dark:hover:bg-blue-500/10 dark:hover:text-blue-400"
          >
            {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          </button>
          <button
            type="button"
            onClick={() => regenerateCode()}
            aria-label={t`重新生成`}
            title={t`重新生成`}
            className="flex size-10 items-center justify-center rounded-[12px] bg-white text-muted-foreground shadow-[inset_0_1px_0_rgba(255,255,255,0.9)] ring-1 ring-black/[0.05] transition-[background-color,color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-zinc-100 hover:text-foreground active:scale-[0.98] dark:bg-zinc-950/70 dark:ring-white/10 dark:hover:bg-white/[0.08]"
          >
            <RefreshCw className="size-3.5" />
          </button>
        </div>

        <button
          type="button"
          onClick={() => setInputOpen(true)}
          className="group mt-2 flex w-full min-w-0 items-center gap-2 rounded-[12px] bg-white/75 px-2.5 py-2 text-left ring-1 ring-black/[0.04] transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white active:scale-[0.99] dark:bg-zinc-950/50 dark:ring-white/10 dark:hover:bg-white/[0.06]"
        >
          <span className="flex size-8 shrink-0 items-center justify-center rounded-[10px] bg-blue-50 text-blue-600 ring-1 ring-blue-100 dark:bg-blue-500/15 dark:text-blue-400 dark:ring-blue-400/10">
            <Keyboard className="size-3.5" />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-[13px] font-medium text-foreground">
              <Trans>输入配对码</Trans>
            </span>
            <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
              <Trans>输入另一台设备显示的 6 位数字</Trans>
            </span>
          </span>
          <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-zinc-100 text-muted-foreground transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:translate-x-0.5 group-hover:-translate-y-0.5 group-hover:text-foreground dark:bg-white/[0.06]">
            <ArrowUpRight className="size-3" />
          </span>
        </button>
      </div>

      <PairingInputDialog open={inputOpen} onOpenChange={setInputOpen} />
    </SectionShell>
  );
}

function NearbyDeviceRow({
  device,
  onSend,
  onConnect,
}: {
  device: Device;
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
}) {
  const DeviceIcon = getDeviceIcon(device.platform);
  const isPaired = device.isPaired;
  const handleClick = () => {
    if (isPaired) {
      onSend(device);
    } else {
      onConnect(device);
    }
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      className="group flex min-w-0 items-center gap-3 rounded-[15px] bg-white/70 p-2.5 text-left ring-1 ring-black/[0.04] transition-[background-color,box-shadow,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white hover:shadow-[0_12px_32px_rgba(37,99,235,0.08)] active:scale-[0.99] dark:bg-zinc-950/45 dark:ring-white/10 dark:hover:bg-white/[0.07]"
    >
      <span className="flex size-9 shrink-0 items-center justify-center rounded-[13px] bg-blue-50 text-blue-600 ring-1 ring-blue-100 transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:scale-105 dark:bg-blue-500/15 dark:text-blue-400 dark:ring-blue-400/10">
        <DeviceIcon className="size-4.5" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium text-foreground">
          {deviceDisplayName(device)}
        </span>
        <span className="mt-0.5 flex items-center gap-1.5 text-xs text-muted-foreground">
          <Wifi className="size-3" />
          {isPaired ? <Trans>已配对</Trans> : <Trans>可配对</Trans>}
        </span>
      </span>
      <span
        className={cn(
          "shrink-0 rounded-full px-3 py-1.5 text-xs font-medium shadow-[0_8px_18px_rgba(37,99,235,0.18)]",
          isPaired
            ? "bg-zinc-950 text-white dark:bg-white dark:text-zinc-950"
            : "bg-blue-600 text-white",
        )}
      >
        {isPaired ? <Trans>发送</Trans> : <Trans>配对</Trans>}
      </span>
    </button>
  );
}

function NearbyFilterControl({
  value,
  onChange,
}: {
  value: NearbyFilter;
  onChange: (value: NearbyFilter) => void;
}) {
  return (
    <div className="flex shrink-0 rounded-full bg-white/80 p-0.5 ring-1 ring-black/[0.04] dark:bg-zinc-950/55 dark:ring-white/10">
      {nearbyFilterOptions.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={value === option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            "rounded-full px-2 py-1 text-[11px] font-medium transition-[background-color,color] duration-200",
            value === option.value
              ? "bg-zinc-950 text-white dark:bg-white dark:text-zinc-950"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function PairingInputDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const openInput = usePairingStore((state) => state.openInput);
  const reset = usePairingStore((state) => state.reset);
  const searchDevice = usePairingStore((state) => state.searchDevice);
  const sendPairingRequest = usePairingStore((state) => state.sendPairingRequest);
  const isSearching = usePairingStore(
    (state) => state.current.phase === "searching",
  );
  const { showDeviceFound, deviceInfo, isRequesting } = useDeviceFoundState();
  const [code, setCode] = useState("");

  const handleOpenChange = (nextOpen: boolean) => {
    if (nextOpen) {
      setCode("");
      openInput();
    } else {
      reset();
      setCode("");
    }
    onOpenChange(nextOpen);
  };

  const handleCodeComplete = (value: string) => {
    if (value.length === 6) {
      searchDevice(value);
    }
  };

  const handleConfirm = () => {
    if (code.length === 6) {
      searchDevice(code);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-[460px]">
        {showDeviceFound && deviceInfo ? (
          <DesktopDeviceFoundContent
            deviceInfo={deviceInfo}
            isRequesting={isRequesting}
            onSendRequest={() => sendPairingRequest()}
            onCancel={() => {
              reset();
              setCode("");
            }}
          />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>
                <Trans>输入配对码</Trans>
              </DialogTitle>
              <DialogDescription>
                <Trans>输入另一台设备上显示的 6 位数字配对码。</Trans>
              </DialogDescription>
            </DialogHeader>

            <div className="flex flex-col items-center gap-4 py-2">
              <InputOTP
                maxLength={6}
                value={code}
                onChange={setCode}
                onComplete={handleCodeComplete}
                disabled={isSearching}
                autoFocus
              >
                <InputOTPGroup>
                  <InputOTPSlot index={0} className="h-12 w-10 text-xl font-semibold" />
                  <InputOTPSlot index={1} className="h-12 w-10 text-xl font-semibold" />
                  <InputOTPSlot index={2} className="h-12 w-10 text-xl font-semibold" />
                </InputOTPGroup>
                <InputOTPSeparator />
                <InputOTPGroup>
                  <InputOTPSlot index={3} className="h-12 w-10 text-xl font-semibold" />
                  <InputOTPSlot index={4} className="h-12 w-10 text-xl font-semibold" />
                  <InputOTPSlot index={5} className="h-12 w-10 text-xl font-semibold" />
                </InputOTPGroup>
              </InputOTP>

              {isSearching && (
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Loader2 className="size-4 animate-spin" />
                  <Trans>正在查找设备...</Trans>
                </div>
              )}
            </div>

            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => handleOpenChange(false)}
                className="rounded-md border border-border px-4 py-2 text-sm font-medium text-foreground transition-colors hover:bg-accent"
              >
                <Trans>取消</Trans>
              </button>
              <button
                type="button"
                onClick={handleConfirm}
                disabled={code.length < 6 || isSearching}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:pointer-events-none disabled:opacity-50"
              >
                <Trans>确认</Trans>
              </button>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function PairedDevicesSection({
  devices,
  onSend,
  onConnect,
  onUnpair,
}: {
  devices: Device[];
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
  onUnpair: (device: Device) => void;
}) {
  return (
    <SectionShell>
      <SectionHeader
        title={<Trans>已配对设备</Trans>}
        count={devices.length}
        description={<Trans>在线设备优先显示，可直接进入发送流程。</Trans>}
      />
      {devices.length === 0 ? (
        <EmptyPanel
          title={<Trans>还没有已配对设备</Trans>}
          description={<Trans>从附近设备发起配对，或使用配对码连接另一台设备。</Trans>}
        />
      ) : (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {devices.map((device) => (
            <DeviceCard
              key={device.peerId}
              device={device}
              onSend={onSend}
              onConnect={onConnect}
              onUnpair={onUnpair}
            />
          ))}
        </div>
      )}
    </SectionShell>
  );
}

function ActiveTransfersSection({ sessionIds }: { sessionIds: string[] }) {
  return (
    <SectionShell>
      <SectionHeader
        title={<Trans>正在传输</Trans>}
        count={sessionIds.length}
        icon={Send}
        description={<Trans>当前会话完成后会进入历史记录。</Trans>}
      />
      {sessionIds.length === 0 ? (
        <EmptyPanel
          title={<Trans>暂无正在传输</Trans>}
          description={<Trans>开始发送或接收文件后，当前任务会显示在这里。</Trans>}
          className="py-4"
        />
      ) : (
        <div className="flex flex-col gap-2.5">
          {sessionIds.map((sessionId) => (
            <TransferItem key={sessionId} sessionId={sessionId} />
          ))}
        </div>
      )}
    </SectionShell>
  );
}
