/**
 * Devices Page (Lazy)
 * 设备页面 - 懒加载组件
 * 根据断点渲染移动端/桌面端不同布局
 */

import { useMemo, useState } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { DeviceCard } from "./-components/device-card";
import type { Device } from "@/commands/network";
import { Trans } from "@lingui/react/macro";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePairingStore } from "@/stores/pairing-store";
import { useBreakpoint } from "@/hooks/use-breakpoint";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { removePairedDevice } from "@/commands/pairing";
import { AddDeviceMenu } from "./-components/add-device-menu";
import { NetworkStatusBar } from "./-components/network-status-bar";
import { OfflineEmptyState } from "./-components/offline-empty-state";
import { StartNodeSheet } from "@/components/network/start-node-sheet";
import { StopNodeSheet } from "@/components/network/stop-node-sheet";

export const Route = createLazyFileRoute("/_app/devices/")({
  component: DevicesPage,
});

function DevicesPage() {
  const breakpoint = useBreakpoint();
  const isMobile = breakpoint === "mobile";
  const navigate = useNavigate();

  const devices = useNetworkStore((s) => s.devices);
  const status = useNetworkStore((s) => s.status);
  const isOnline = status === "running" || status === "starting";
  const storedPairedDevices = useSecretStore((state) => state.pairedDevices);
  const directPairing = usePairingStore((state) => state.directPairing);

  // directPairing 成功后自动跳转到设备页面（刷新列表）
  usePairingSuccess();

  // 节点控制弹窗状态
  const [startSheetOpen, setStartSheetOpen] = useState(false);
  const [stopSheetOpen, setStopSheetOpen] = useState(false);

  // 已配对设备：后端在线数据优先，离线回退到 secret-store
  const pairedDevices = useMemo<Device[]>(() => {
    const deviceMap = new Map(devices.map((d) => [d.peerId, d]));
    return storedPairedDevices.map((stored) => {
      const backendDevice = deviceMap.get(stored.peerId);
      if (backendDevice) {
        return backendDevice;
      }
      // 节点未运行或设备离线，用 secret-store 数据显示为离线
      return {
        peerId: stored.peerId,
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

  // 附近设备：后端返回的未配对设备
  const filteredNearbyDevices = useMemo(() => {
    return devices.filter((d) => !d.isPaired);
  }, [devices]);

  const handleSend = (device: Device) => {
    navigate({ to: "/send", search: { peerId: device.peerId } });
  };

  const handleConnect = (device: Device) => {
    directPairing(device.peerId);
  };

  const handleUnpair = (device: Device) => {
    // 同时更新后端运行时状态（节点未运行时静默成功）
    removePairedDevice(device.peerId);
    useSecretStore.getState().removePairedDevice(device.peerId);
  };

  return (
    <>
      {isMobile ? (
        <MobileDevicesView
          isOnline={isOnline}
          pairedDevices={pairedDevices}
          nearbyDevices={filteredNearbyDevices}
          onSend={handleSend}
          onConnect={handleConnect}
          onUnpair={handleUnpair}
          onStartClick={() => setStartSheetOpen(true)}
          onStopClick={() => setStopSheetOpen(true)}
          onStatusClick={() => status === "running" ? setStopSheetOpen(true) : setStartSheetOpen(true)}
        />
      ) : (
        <DesktopDevicesView
          isOnline={isOnline}
          pairedDevices={pairedDevices}
          nearbyDevices={filteredNearbyDevices}
          onSend={handleSend}
          onConnect={handleConnect}
          onUnpair={handleUnpair}
          onStartClick={() => setStartSheetOpen(true)}
          onStopClick={() => setStopSheetOpen(true)}
          onStatusClick={() => status === "running" ? setStopSheetOpen(true) : setStartSheetOpen(true)}
        />
      )}

      {/* 节点控制弹窗 */}
      <StartNodeSheet open={startSheetOpen} onOpenChange={setStartSheetOpen} />
      <StopNodeSheet open={stopSheetOpen} onOpenChange={setStopSheetOpen} />
    </>
  );
}

/* ─────────────────── 共享类型 ─────────────────── */

interface DevicesViewProps {
  isOnline: boolean;
  pairedDevices: Device[];
  nearbyDevices: Device[];
  onSend: (device: Device) => void;
  onConnect: (device: Device) => void;
  onUnpair: (device: Device) => void;
  onStartClick: () => void;
  onStopClick: () => void;
  onStatusClick: () => void;
}

/* ─────────────────── 移动端视图 ─────────────────── */

function MobileDevicesView({
  isOnline,
  pairedDevices,
  nearbyDevices,
  onSend,
  onConnect,
  onUnpair,
  onStartClick,
  onStopClick,
  onStatusClick,
}: DevicesViewProps) {
  return (
    <main className="flex h-full flex-1 flex-col bg-background">
      {/* 网络状态条 */}
      <div className="px-4 pt-3">
        <NetworkStatusBar onStopClick={onStopClick} onStatusClick={onStatusClick} />
      </div>

      {/* 内容区域 */}
      {isOnline ? (
        <div className="flex-1 overflow-auto px-4 py-4">
          <div className="flex flex-col gap-5">
            {/* 已配对设备 */}
            {pairedDevices.length > 0 && (
              <section className="flex flex-col gap-3">
                <div className="flex items-center gap-1.5">
                  <h2 className="text-[15px] font-semibold text-foreground">
                    <Trans>已配对设备</Trans>
                  </h2>
                  <span className="text-[13px] text-muted-foreground">
                    ({pairedDevices.length})
                  </span>
                </div>
                <div className="flex flex-col gap-2.5">
                  {pairedDevices.map((device) => (
                    <DeviceCard
                      key={device.peerId}
                      device={device}
                      variant="list"
                      onSend={onSend}
                      onConnect={onConnect}
                      onUnpair={onUnpair}
                    />
                  ))}
                </div>
              </section>
            )}

            {/* 附近设备 */}
            {nearbyDevices.length > 0 && (
              <section className="flex flex-col gap-3">
                <div className="flex items-center gap-1.5">
                  <h2 className="text-[15px] font-semibold text-foreground">
                    <Trans>附近设备</Trans>
                  </h2>
                  <span className="text-[13px] text-muted-foreground">
                    ({nearbyDevices.length})
                  </span>
                </div>
                <div className="flex flex-col gap-2.5">
                  {nearbyDevices.map((device) => (
                    <DeviceCard
                      key={device.peerId}
                      device={device}
                      variant="list"
                      onSend={onSend}
                      onConnect={onConnect}
                    />
                  ))}
                </div>
              </section>
            )}
          </div>
        </div>
      ) : (
        <OfflineEmptyState onStartClick={onStartClick} />
      )}
    </main>
  );
}

/* ─────────────────── 桌面端视图 ─────────────────── */

function DesktopDevicesView({
  isOnline,
  pairedDevices,
  nearbyDevices,
  onSend,
  onConnect,
  onUnpair,
  onStartClick,
  onStopClick,
  onStatusClick,
}: DevicesViewProps) {
  return (
    <main className="flex h-full flex-1 flex-col bg-background">
      {/* Toolbar */}
      <header className="flex h-13 items-center justify-between border-b border-border px-4 lg:px-5">
        <div className="flex items-center gap-2">
          <h1 className="text-[15px] font-medium text-foreground">
            <Trans>设备</Trans>
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <AddDeviceMenu />
        </div>
      </header>

      {isOnline ? (
        /* Page Content */
        <div className="flex-1 overflow-auto p-5 lg:p-6">
          <div className="flex flex-col gap-6">
            {/* 网络状态栏 */}
            <NetworkStatusBar onStopClick={onStopClick} onStatusClick={onStatusClick} />

            {/* Paired Devices Section */}
            <section className="flex flex-col gap-4">
              <div className="flex items-center gap-2">
                <h2 className="text-sm font-semibold text-foreground">
                  <Trans>已配对设备</Trans>
                </h2>
                <span className="text-[13px] text-muted-foreground">
                  ({pairedDevices.length})
                </span>
              </div>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
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

            {/* Divider */}
            <div className="h-px bg-border" />

            {/* Nearby Devices Section */}
            <section className="flex flex-col gap-4">
              <div className="flex items-center gap-2">
                <h2 className="text-sm font-semibold text-foreground">
                  <Trans>附近设备</Trans>
                </h2>
                <span className="text-[13px] text-muted-foreground">
                  ({nearbyDevices.length})
                </span>
              </div>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
                {nearbyDevices.map((device) => (
                  <DeviceCard
                    key={device.peerId}
                    device={device}
                    onSend={onSend}
                    onConnect={onConnect}
                  />
                ))}
              </div>
            </section>
          </div>
        </div>
      ) : (
        <OfflineEmptyState onStartClick={onStartClick} />
      )}
    </main>
  );
}
