/**
 * Add Device Section（桌面端）
 * 添加设备区块 —— 附近设备行、本机配对码区，以及输入配对码弹窗。
 * 从 devices/index.lazy.tsx 抽出，设备页主文件只负责编排各区块。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import {
  ArrowUpRight,
  Check,
  Clock,
  Copy,
  Keyboard,
  Link as LinkIcon,
  Loader2,
  Radio,
  RefreshCw,
  Wifi,
} from "lucide-react";

import type { Device } from "@/lib/bindings";
import { useNetworkStore } from "@/stores/network-store";
import { usePairingStore } from "@/stores/pairing-store";
import { useCountdown } from "@/hooks/use-countdown";
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
import { SectionHeader, SectionShell } from "@/components/layout/section-primitives";

type NearbyFilter = "all" | "unpaired" | "paired";

const nearbyFilterOptions: Array<{
  value: NearbyFilter;
  label: React.ReactNode;
}> = [
  { value: "all", label: <Trans>全部</Trans> },
  { value: "unpaired", label: <Trans>可配对</Trans> },
  { value: "paired", label: <Trans>已配对</Trans> },
];

export function AddDeviceSection({
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
  const nodeStatus = useNetworkStore((state) => state.status);
  const isNodeRunning = nodeStatus === "running";
  const isNodeStarting = nodeStatus === "starting";
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
  const isFilteredEmpty = devices.length > 0 && filteredDevices.length === 0;

  useEffect(() => {
    if (isNodeRunning) {
      ensureActiveCode();
    }
  }, [ensureActiveCode, isNodeRunning]);

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

  const isLoading =
    isNodeStarting ||
    (isNodeRunning && codeInfo === null && errorMessage === null);

  return (
    <SectionShell className="gap-3.5">
      <SectionHeader
        title={<Trans>添加设备</Trans>}
        count={devices.length}
        icon={LinkIcon}
        description={<Trans>附近设备优先，已配对设备可直接发送。</Trans>}
      />

      <div className="space-y-2.5">
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
          <div className="rounded-[15px] bg-foreground/[0.035] px-3 py-3 shadow-[inset_0_1px_0_rgba(255,255,255,0.38)] dark:bg-white/[0.045] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.07)]">
            <p className="text-sm font-medium text-foreground">
              {isFilteredEmpty ? (
                <Trans>没有符合条件的附近设备</Trans>
              ) : (
                <Trans>暂无附近设备</Trans>
              )}
            </p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {isFilteredEmpty ? (
                <Trans>切换过滤条件，或直接使用下方配对码连接。</Trans>
              ) : (
                <Trans>确认对端已启动，或直接使用下方配对码连接。</Trans>
              )}
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-2">
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

      <div className="h-px bg-foreground/[0.055] dark:bg-white/[0.075]" />

      <div className="glass-accent overflow-hidden rounded-[16px] p-2.5">
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
            <div className="glass-control flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[11px] text-muted-foreground">
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
          <div className="glass-control flex h-10 min-w-0 items-center justify-center rounded-[12px] px-2.5">
            {isNodeStarting ? (
              <span className="text-[11px] text-muted-foreground">
                <Trans>等待节点启动</Trans>
              </span>
            ) : isLoading ? (
              <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
            ) : isNodeRunning && errorMessage ? (
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
            disabled={!isNodeRunning || !codeInfo || isExpired}
            aria-label={copied ? t`已复制` : t`复制配对码`}
            title={copied ? t`已复制` : t`复制配对码`}
            className="glass-control flex size-10 items-center justify-center rounded-[12px] text-muted-foreground transition-[background-color,color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:text-brand active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 dark:hover:text-brand"
          >
            {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          </button>
          <button
            type="button"
            onClick={() => {
              if (isNodeRunning) {
                regenerateCode();
              }
            }}
            disabled={!isNodeRunning}
            aria-label={t`重新生成`}
            title={t`重新生成`}
            className="glass-control flex size-10 items-center justify-center rounded-[12px] text-muted-foreground transition-[background-color,color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:text-foreground active:scale-[0.98]"
          >
            <RefreshCw className="size-3.5" />
          </button>
        </div>

        <button
          type="button"
          onClick={() => setInputOpen(true)}
          className="group mt-2 flex w-full min-w-0 items-center gap-2 rounded-[12px] bg-white/38 px-2.5 py-2 text-left shadow-[inset_0_1px_0_rgba(255,255,255,0.38)] transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/52 active:scale-[0.99] dark:bg-white/[0.045] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)] dark:hover:bg-white/[0.065]"
        >
          <span className="flex size-8 shrink-0 items-center justify-center rounded-[10px] bg-primary/10 text-brand ring-1 ring-primary/15 dark:bg-primary/15 dark:ring-primary/10">
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
          <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-foreground/[0.045] text-muted-foreground transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:translate-x-0.5 group-hover:-translate-y-0.5 group-hover:text-foreground dark:bg-white/[0.06]">
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
      className="group flex min-w-0 items-center gap-3 rounded-[15px] bg-white/35 p-2.5 text-left shadow-[inset_0_1px_0_rgba(255,255,255,0.34)] transition-[background-color,box-shadow,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/55 hover:shadow-[0_12px_32px_rgba(219,163,65,0.07),inset_0_1px_0_rgba(255,255,255,0.5)] active:scale-[0.99] dark:bg-white/[0.045] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)] dark:hover:bg-white/[0.07]"
    >
      <span className="flex size-9 shrink-0 items-center justify-center rounded-[13px] bg-primary/10 text-brand ring-1 ring-primary/15 transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:scale-105 dark:bg-primary/15 dark:ring-primary/10">
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
          "shrink-0 rounded-full px-3 py-1.5 text-xs font-medium shadow-[0_8px_18px_rgba(219,163,65,0.18)]",
          isPaired
            ? "bg-zinc-950 text-white dark:bg-primary/20 dark:text-brand dark:ring-1 dark:ring-primary/20"
            : "bg-primary text-primary-foreground",
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
    <div className="flex shrink-0 rounded-full bg-foreground/[0.045] p-0.5 dark:bg-white/[0.06]">
      {nearbyFilterOptions.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={value === option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            "rounded-full px-2 py-1 text-[11px] font-medium transition-[background-color,color] duration-200",
            value === option.value
              ? "bg-zinc-950 text-white dark:bg-primary/20 dark:text-brand"
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
  const phase = usePairingStore((state) => state.current.phase);
  const { showDeviceFound, deviceInfo, isRequesting } = useDeviceFoundState();
  const [code, setCode] = useState("");

  // 配对成功后由 usePairingSuccess 统一 navigate+reset；这里只负责关闭本地内联弹窗并
  // 清掉已被消费的验证码，否则成功后弹窗会停在已失效的 OTP 输入界面（再次确认会报错）。
  useEffect(() => {
    if (phase === "success") {
      setCode("");
      onOpenChange(false);
    }
  }, [phase, onOpenChange]);

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
                className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50"
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
