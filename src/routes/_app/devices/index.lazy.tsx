/**
 * Devices Page (Lazy)
 * 桌面端主屏 —— 聚合「我的设备」网格 + 最近传输
 * 移动端已迁移到 SwarmDrop-RN,此处仅桌面端
 */

import { useMemo, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { DeviceCard } from "./-components/device-card";
import type { Device } from "@/lib/bindings";
import { Trans } from "@lingui/react/macro";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePairingStore } from "@/stores/pairing-store";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { commands } from "@/lib/bindings";
import { OfflineEmptyState } from "./-components/offline-empty-state";
import { AddDeviceMenu } from "./-components/add-device-menu";
import { HomeRecentTransfers } from "./-components/home-recent-transfers";
import { StartNodeSheet } from "@/components/network/start-node-sheet";
import { StopNodeSheet } from "@/components/network/stop-node-sheet";

export const Route = createLazyFileRoute("/_app/devices/")({
  component: DevicesPage,
});

function DevicesPage() {
  const navigate = useNavigate();

  const devices = useNetworkStore((s) => s.devices);
  const status = useNetworkStore((s) => s.status);
  const isOnline = status === "running" || status === "starting";
  const storedPairedDevices = useSecretStore((state) => state.pairedDevices);
  const directPairing = usePairingStore((state) => state.directPairing);

  // directPairing 成功后自动跳转到设备页面(刷新列表)
  usePairingSuccess();

  // 节点控制弹窗状态
  const [startSheetOpen, setStartSheetOpen] = useState(false);
  const [stopSheetOpen, setStopSheetOpen] = useState(false);

  // 已配对设备:后端在线数据优先,离线回退到 secret-store
  const pairedDevices = useMemo<Device[]>(() => {
    const deviceMap = new Map(devices.map((d) => [d.peerId, d]));
    return storedPairedDevices.map((stored) => {
      const backendDevice = deviceMap.get(stored.peerId);
      if (backendDevice) {
        return backendDevice;
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
    });
  }, [storedPairedDevices, devices]);

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
        pairedDevices={pairedDevices}
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
  pairedDevices: Device[];
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
  onUnpair: (device: Device) => void;
  onStartClick: () => void;
}

function DesktopDevicesView({
  isOnline,
  pairedDevices,
  onSend,
  onConnect,
  onUnpair,
  onStartClick,
}: DesktopDevicesViewProps) {
  // 桌面端主屏:拖放区 / 我的设备网格 / 最近传输 —— 顶栏由全局 AppTopBar 承载
  return (
    <main className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      {isOnline ? (
        <div className="flex-1 overflow-auto">
          <div className="mx-auto flex w-full max-w-[1080px] flex-col gap-7 px-8 py-8">
            <section className="flex flex-col gap-3">
              <div className="flex items-center gap-2 pb-1">
                <h2 className="text-sm font-semibold text-foreground">
                  <Trans>我的设备</Trans>
                </h2>
                <span className="rounded-full bg-secondary px-2 py-0.5 text-[11px] font-semibold text-muted-foreground">
                  {pairedDevices.length}
                </span>
                <div className="ml-auto">
                  <AddDeviceMenu variant="default" />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-4">
                {pairedDevices.map((device) => (
                  <DeviceCard
                    key={device.peerId}
                    device={device}
                    onSend={onSend}
                    onConnect={onConnect}
                    onUnpair={onUnpair}
                  />
                ))}
              </div>
            </section>

            <HomeRecentTransfers />
          </div>
        </div>
      ) : (
        <OfflineEmptyState onStartClick={onStartClick} />
      )}
    </main>
  );
}
