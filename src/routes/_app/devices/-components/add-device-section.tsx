/**
 * Add Device Section（桌面端）
 * 添加设备区块 —— 附近设备行 + 配对入口（跳转到 canonical 配对路由）。
 *
 * 配对 UI（生成邀请 / 粘贴邀请）统一在 `/pairing/generate` `/pairing/input` 路由，
 * 本区块只负责附近设备列表 + 两个入口按钮，避免内嵌重复实现（DRY）。
 */

import { useMemo, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { useNavigate } from "@tanstack/react-router";
import { ArrowUpRight, ClipboardPaste, Link as LinkIcon, QrCode, Radio, Wifi } from "lucide-react";

import type { Device } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { deviceDisplayName } from "@/lib/device-name";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import {
  SectionHeader,
  SectionShell,
  SegmentedControl,
} from "@/components/layout/section-primitives";

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
  const navigate = useNavigate();
  const [nearbyFilter, setNearbyFilter] = useState<NearbyFilter>("all");

  const filteredDevices = useMemo(() => {
    if (nearbyFilter === "paired") return devices.filter((d) => d.isPaired);
    if (nearbyFilter === "unpaired") return devices.filter((d) => !d.isPaired);
    return devices;
  }, [devices, nearbyFilter]);
  const isFilteredEmpty = devices.length > 0 && filteredDevices.length === 0;

  return (
    <SectionShell data-testid="add-device-section" className="gap-3.5">
      <SectionHeader
        title={<Trans>添加设备</Trans>}
        count={devices.length}
        icon={LinkIcon}
        description={<Trans>附近设备优先，或通过邀请连接跨网设备。</Trans>}
      />

      <div className="space-y-2.5">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <Radio className="size-3.5" />
            <Trans>附近设备</Trans>
          </div>
          <SegmentedControl<NearbyFilter>
            value={nearbyFilter}
            options={nearbyFilterOptions}
            onChange={setNearbyFilter}
          />
        </div>

        {filteredDevices.length === 0 ? (
          <div
            data-testid="nearby-devices-empty"
            className="rounded-[15px] bg-foreground/[0.035] px-3 py-3 shadow-[inset_0_1px_0_rgba(255,255,255,0.38)] dark:bg-white/[0.045] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.07)]"
          >
            <p className="text-sm font-medium text-foreground">
              {isFilteredEmpty ? (
                <Trans>没有符合条件的附近设备</Trans>
              ) : (
                <Trans>暂无附近设备</Trans>
              )}
            </p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {isFilteredEmpty ? (
                <Trans>切换过滤条件，或通过邀请连接设备。</Trans>
              ) : (
                <Trans>确认对端已启动，或通过邀请连接设备。</Trans>
              )}
            </p>
          </div>
        ) : (
          <div data-testid="nearby-devices-list" className="flex flex-col gap-2">
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

      {/* 配对入口——跳转到 canonical 路由 */}
      <div className="grid grid-cols-2 gap-2">
        {/* 文案与配对页的模式切换同名，侧栏两列也放得下不截断 */}
        <PairingEntryButton
          icon={QrCode}
          testid="pairing-generate-action"
          title={<Trans>展示邀请</Trans>}
          subtitle={<Trans>让对方扫码</Trans>}
          onClick={() => navigate({ to: "/pairing/generate" })}
        />
        <PairingEntryButton
          icon={ClipboardPaste}
          testid="pairing-input-action"
          title={<Trans>粘贴邀请</Trans>}
          subtitle={<Trans>已收到邀请</Trans>}
          onClick={() => navigate({ to: "/pairing/input" })}
        />
      </div>
    </SectionShell>
  );
}

function PairingEntryButton({
  icon: Icon,
  testid,
  title,
  subtitle,
  onClick,
}: {
  icon: React.ComponentType<{ className?: string }>;
  testid: string;
  title: React.ReactNode;
  subtitle: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      data-testid={testid}
      className="group flex min-w-0 items-center gap-2 rounded-[12px] bg-white/38 px-2.5 py-2 text-left shadow-[inset_0_1px_0_rgba(255,255,255,0.38)] transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/52 focus-ring active:scale-[0.99] motion-reduce:transition-none dark:bg-white/[0.045] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)] dark:hover:bg-white/[0.065]"
    >
      <span className="flex size-8 shrink-0 items-center justify-center rounded-[10px] bg-primary/10 text-brand ring-1 ring-primary/15 dark:bg-primary/15 dark:ring-primary/10">
        <Icon className="size-3.5" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-medium text-foreground">{title}</span>
        <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">{subtitle}</span>
      </span>
      <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-foreground/[0.045] text-muted-foreground transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:translate-x-0.5 group-hover:-translate-y-0.5 group-hover:text-foreground dark:bg-white/[0.06]">
        <ArrowUpRight className="size-3" />
      </span>
    </button>
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
    if (isPaired) onSend(device);
    else onConnect(device);
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      data-testid="nearby-device-row"
      data-peer-id={device.peerId}
      data-device-paired={isPaired ? "true" : "false"}
      className="group flex min-w-0 items-center gap-3 rounded-[15px] bg-white/35 p-2.5 text-left shadow-[inset_0_1px_0_rgba(255,255,255,0.34)] transition-[background-color,box-shadow,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/55 hover:shadow-[0_12px_32px_rgb(8_121_104_/_0.08),inset_0_1px_0_rgba(255,255,255,0.5)] focus-ring active:scale-[0.99] motion-reduce:transition-none dark:bg-white/[0.045] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)] dark:hover:bg-white/[0.07]"
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
          "shrink-0 rounded-full px-3 py-1.5 text-xs font-medium shadow-[0_8px_18px_rgb(8_121_104_/_0.18)]",
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
