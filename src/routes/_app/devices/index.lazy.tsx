/**
 * Devices Page (Lazy)
 * 桌面端主屏 —— 设备发现、快速配对、已配对设备和活跃传输
 * 移动端已迁移到 SwarmDrop-RN,此处仅桌面端
 */

import { useCallback, useMemo, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { DeviceCard } from "./-components/device-card";
import { AddDeviceSection } from "./-components/add-device-section";
import {
  EmptyPanel,
  SectionHeader,
  SectionShell,
} from "@/components/layout/section-primitives";
import { SessionRow } from "../transfer/-session-row";
import type {
  Device,
  DeviceReceivePolicy,
  DeviceTrustLevel,
  TransferProjection,
} from "@/lib/bindings";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePairingStore } from "@/stores/pairing-store";
import { useTransferStore } from "@/stores/transfer-store";
import { isProjectionActive } from "@/lib/transfer-projection";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { commands } from "@/lib/bindings";
import { OfflineEmptyState } from "./-components/offline-empty-state";
import { StartNodeSheet } from "@/components/network/start-node-sheet";
import { StopNodeSheet } from "@/components/network/stop-node-sheet";
import { deviceDisplayName } from "@/lib/device-name";
import { MonitorSmartphone, Send } from "lucide-react";

export const Route = createLazyFileRoute("/_app/devices/")({
  component: DevicesPage,
});

function DevicesPage() {
  const navigate = useNavigate();

  const devices = useNetworkStore((s) => s.devices);
  const status = useNetworkStore((s) => s.status);
  const fetchDevices = useNetworkStore((s) => s.fetchDevices);
  const isOnline = status === "running" || status === "starting";
  const storedPairedDevices = useSecretStore((state) => state.pairedDevices);
  const removePairedDevice = useSecretStore((state) => state.removePairedDevice);
  const upsertPairedDevice = useSecretStore((state) => state.upsertPairedDevice);
  const directPairing = usePairingStore((state) => state.directPairing);
  const projections = useTransferStore((s) => s.projections);

  // directPairing 成功后自动跳转到设备页面(刷新列表)
  usePairingSuccess();

  // 节点控制弹窗状态
  const [startSheetOpen, setStartSheetOpen] = useState(false);
  const [stopSheetOpen, setStopSheetOpen] = useState(false);

  // 已配对设备:后端在线数据优先,离线回退到 secret-store。
  // stored(来自 storedPairedDevices)存在即等价于"已配对",无需再单独维护 pairedPeerIds 集。
  const normalizedDevices = useMemo<Device[]>(() => {
    const storedMap = new Map(storedPairedDevices.map((d) => [d.peerId, d]));
    return devices.map((device) => {
      const stored = storedMap.get(device.peerId);
      return stored
        ? {
            ...device,
            isPaired: true,
            trustLevel: device.trustLevel ?? stored.trustLevel ?? "collaborator",
            receivePolicy: device.receivePolicy ?? stored.receivePolicy ?? null,
            trustConfirmed: device.trustConfirmed ?? stored.trustConfirmed ?? false,
          }
        : device;
    });
  }, [devices, storedPairedDevices]);

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
          capabilities: stored.capabilities ?? [],
          status: "offline" as const,
          connection: null,
          latency: null,
          isPaired: true,
          trustLevel: stored.trustLevel ?? "collaborator",
          receivePolicy: stored.receivePolicy ?? null,
          trustConfirmed: stored.trustConfirmed ?? false,
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

  const activeItems = useMemo(
    () =>
      Object.values(projections)
        .filter(isProjectionActive)
        .sort((a, b) => b.startedAt - a.startedAt),
    [projections],
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
    removePairedDevice(device.peerId);
  };

  const handleUpdatePolicy = useCallback(
    async (
      device: Device,
      trustLevel: DeviceTrustLevel,
      receivePolicy: DeviceReceivePolicy,
    ) => {
      const updated = await commands.updatePairedDevicePolicy(
        device.peerId,
        trustLevel,
        receivePolicy,
      );
      upsertPairedDevice(updated);
      await fetchDevices("all");
      toast.success(t`已更新可信设备策略`);
    },
    [fetchDevices, upsertPairedDevice],
  );

  return (
    <>
      <DesktopDevicesView
        isOnline={isOnline}
        nearbyDevices={nearbyDevices}
        pairedDevices={pairedDevices}
        activeItems={activeItems}
        onSend={handleSend}
        onConnect={handleConnect}
        onUnpair={handleUnpair}
        onUpdatePolicy={handleUpdatePolicy}
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
  activeItems: TransferProjection[];
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
  onUnpair: (device: Device) => void;
  onUpdatePolicy: (
    device: Device,
    trustLevel: DeviceTrustLevel,
    receivePolicy: DeviceReceivePolicy,
  ) => Promise<void>;
  onStartClick: () => void;
}

function DesktopDevicesView({
  isOnline,
  nearbyDevices,
  pairedDevices,
  activeItems,
  onSend,
  onConnect,
  onUnpair,
  onUpdatePolicy,
  onStartClick,
}: DesktopDevicesViewProps) {
  // 桌面端主屏:设备发现 / 快速配对 / 已配对设备 / 活跃传输 —— 顶栏由全局 AppTopBar 承载
  return (
    <main
      data-testid="desktop-devices-page"
      className="flex h-full flex-1 flex-col overflow-hidden bg-transparent"
    >
      {isOnline ? (
        <div className="flex-1 overflow-auto bg-transparent">
          <div className="mx-auto grid w-full max-w-[1220px] gap-5 px-5 py-5 min-[920px]:grid-cols-[minmax(0,1fr)_360px] lg:grid-cols-[minmax(0,1fr)_380px] lg:px-8 lg:py-7">
            <HomeOverview
              nearbyCount={nearbyDevices.length}
              pairedCount={pairedDevices.length}
              activeCount={activeItems.length}
            />

            <div className="flex min-w-0 flex-col gap-5">
              <PairedDevicesSection
                devices={pairedDevices}
                onSend={onSend}
                onConnect={onConnect}
                onUnpair={onUnpair}
                onUpdatePolicy={onUpdatePolicy}
              />
              <ActiveTransfersSection items={activeItems} />
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
    <section data-testid="desktop-home-overview" className="min-[920px]:col-span-2">
      <div className="glass-panel flex flex-col gap-4 rounded-[24px] px-6 py-5 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-xs font-medium text-brand">
            <span className="flex size-7 items-center justify-center rounded-full bg-primary/10 dark:bg-primary/15">
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
          <OverviewStat
            label={<Trans>附近</Trans>}
            value={nearbyCount}
            testId="desktop-home-stat-nearby"
          />
          <OverviewStat
            label={<Trans>已配对</Trans>}
            value={pairedCount}
            testId="desktop-home-stat-paired"
          />
          <OverviewStat
            label={<Trans>传输中</Trans>}
            value={activeCount}
            testId="desktop-home-stat-active"
          />
        </div>
      </div>
    </section>
  );
}

function OverviewStat({
  label,
  value,
  testId,
}: {
  label: React.ReactNode;
  value: number;
  testId: string;
}) {
  return (
    <div
      data-testid={testId}
      className="rounded-[16px] bg-white/40 px-3 py-2.5 text-center shadow-[inset_0_1px_0_rgba(255,255,255,0.55)] dark:bg-white/[0.055] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.08)]"
    >
      <div className="font-mono text-lg font-semibold text-foreground">
        {value}
      </div>
      <div className="mt-0.5 text-[11px] text-muted-foreground">{label}</div>
    </div>
  );
}

function PairedDevicesSection({
  devices,
  onSend,
  onConnect,
  onUnpair,
  onUpdatePolicy,
}: {
  devices: Device[];
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
  onUnpair: (device: Device) => void;
  onUpdatePolicy: (
    device: Device,
    trustLevel: DeviceTrustLevel,
    receivePolicy: DeviceReceivePolicy,
  ) => Promise<void>;
}) {
  return (
    <SectionShell data-testid="paired-devices-section">
      <SectionHeader
        title={<Trans>已配对设备</Trans>}
        count={devices.length}
        description={<Trans>在线设备优先显示，可直接进入发送流程。</Trans>}
      />
      {devices.length === 0 ? (
        <EmptyPanel
          data-testid="paired-devices-empty"
          title={<Trans>还没有已配对设备</Trans>}
          description={<Trans>从附近设备发起配对，或使用配对码连接另一台设备。</Trans>}
        />
      ) : (
        <div
          data-testid="paired-devices-grid"
          className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3"
        >
          {devices.map((device) => (
            <DeviceCard
              key={device.peerId}
              device={device}
              onSend={onSend}
              onConnect={onConnect}
              onUnpair={onUnpair}
              onUpdatePolicy={onUpdatePolicy}
            />
          ))}
        </div>
      )}
    </SectionShell>
  );
}

function ActiveTransfersSection({ items }: { items: TransferProjection[] }) {
  const navigate = useNavigate();
  const openSession = useCallback(
    (sessionId: string) => {
      void navigate({ to: "/transfer", search: { session: sessionId } });
    },
    [navigate],
  );

  return (
    <SectionShell data-testid="active-transfers-section">
      <SectionHeader
        title={<Trans>正在传输</Trans>}
        count={items.length}
        icon={Send}
        description={<Trans>当前会话完成后会进入历史记录。</Trans>}
      />
      {items.length === 0 ? (
        <EmptyPanel
          data-testid="active-transfers-empty"
          title={<Trans>暂无正在传输</Trans>}
          description={<Trans>开始发送或接收文件后，当前任务会显示在这里。</Trans>}
          className="py-4"
        />
      ) : (
        <div data-testid="active-transfers-list" className="flex flex-col gap-2.5">
          {items.map((item) => (
            <SessionRow
              key={item.sessionId}
              projection={item}
              selected={false}
              onSelect={openSession}
              onSessionChange={openSession}
            />
          ))}
        </div>
      )}
    </SectionShell>
  );
}
