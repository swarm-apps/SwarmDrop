/**
 * Devices Page (Lazy)
 * 设备页面 - 懒加载组件
 * - 移动端:沿用原有"已配对 + 附近"双 section 布局 + 底部 nav
 * - 桌面端:聚合主屏(拖放区 + 我的设备网格 + 最近传输);
 *   附近未配对设备从主屏移除,改由 /pairing 流程承担(directPairing)
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
import { NetworkStatusBar } from "./-components/network-status-bar";
import { OfflineEmptyState } from "./-components/offline-empty-state";
import { AddDeviceMenu } from "./-components/add-device-menu";
import { HomeRecentTransfers } from "./-components/home-recent-transfers";
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
          onSend={handleSend}
          onConnect={handleConnect}
          onUnpair={handleUnpair}
          onStartClick={() => setStartSheetOpen(true)}
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

type DesktopDevicesViewProps = Omit<
  DevicesViewProps,
  "nearbyDevices" | "onStopClick" | "onStatusClick"
>;

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
