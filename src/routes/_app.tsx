/**
 * App Layout
 * 桌面端布局 —— 全局 AppTopBar(非全屏路由)+ 内容区
 * 移动端已迁移到独立 SwarmDrop-RN 项目,此 layout 仅服务桌面壳
 */

import { createFileRoute, Outlet, useLocation } from "@tanstack/react-router";
import { useEffect } from "react";
import {
  AppAmbientBackground,
  AppAmbientLightOverlay,
} from "@/components/layout/app-ambient-background";
import { AppTopBar } from "@/components/layout/app-topbar";
import { useNetworkStore } from "@/stores/network-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { ConnectionRequestDialog } from "@/components/pairing/connection-request-dialog";
import { TransferOfferDialog } from "@/components/transfer/transfer-offer-dialog";
import { ExternalOpenHandler } from "@/components/external-open-handler";
import {
  setupTransferListeners,
  cleanupTransferListeners,
} from "@/stores/transfer-store";
import { isMac } from "@/lib/utils";

export const Route = createFileRoute("/_app")({
  component: AppLayout,
});

function AppLayout() {
  // 传输事件监听
  useEffect(() => {
    setupTransferListeners();
    return () => {
      cleanupTransferListeners();
    };
  }, []);

  // 自动启动节点(解锁后首次进入时检查)
  useEffect(() => {
    const { autoStart } = usePreferencesStore.getState();
    const { status, startNetwork } = useNetworkStore.getState();
    if (autoStart && status === "stopped") {
      void startNetwork().then((ok) => {
        if (!ok) console.warn("[auto-start] 节点自动启动失败");
      });
    }
  }, []);

  const location = useLocation();

  // send/receive/pairing 页面为独立全屏,不显示全局 header
  const isFullScreenRoute =
    location.pathname.startsWith("/send") ||
    location.pathname.startsWith("/receive") ||
    location.pathname.startsWith("/pairing");

  return (
    <div className="app-shell flex h-svh flex-col">
      <AppAmbientBackground />
      {!isFullScreenRoute && <AppTopBar />}
      {isFullScreenRoute && isMac && (
        <div
          data-tauri-drag-region
          className="h-8 shrink-0 bg-background"
        />
      )}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
      <AppAmbientLightOverlay />
      <ConnectionRequestDialog />
      <TransferOfferDialog />
      {/* 入站「用 SwarmDrop 打开」：映射文件 → 选设备屏。命令式，无常驻 UI。 */}
      <ExternalOpenHandler />
    </div>
  );
}
