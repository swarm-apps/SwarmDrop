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
import { AppTopBar, WindowControls } from "@/components/layout/app-topbar";
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
  const autoStart = usePreferencesStore((s) => s.autoStart);
  const networkStatus = useNetworkStore((s) => s.status);
  const startNetwork = useNetworkStore((s) => s.startNetwork);

  // 传输事件监听
  useEffect(() => {
    setupTransferListeners();
    return () => {
      cleanupTransferListeners();
    };
  }, []);

  // 自动启动节点(解锁后首次进入时检查)
  useEffect(() => {
    if (autoStart && networkStatus === "stopped") {
      void startNetwork().then((ok) => {
        if (!ok) console.warn("[auto-start] 节点自动启动失败");
      });
    }
  }, [autoStart, networkStatus, startNetwork]);

  const location = useLocation();

  // send/pairing 页面为独立全屏,不显示全局 header
  const isFullScreenRoute =
    location.pathname.startsWith("/send") ||
    location.pathname.startsWith("/pairing");

  return (
    <div className="app-shell flex h-svh flex-col">
      <AppAmbientBackground />
      {!isFullScreenRoute && <AppTopBar />}
      {/* 全屏路由无 AppTopBar：Windows/Linux 是无边框自绘窗口
          (setup.rs set_decorations(false))，需补一条可拖拽、带窗口控制的玻璃顶条；
          macOS 走系统 Overlay 红绿灯，只留等高拖拽占位。 */}
      {isFullScreenRoute &&
        (isMac ? (
          <div data-tauri-drag-region className="h-8 shrink-0 bg-background" />
        ) : (
          <div
            data-tauri-drag-region
            className="flex h-9 shrink-0 items-center justify-end bg-white/[0.16] pr-2 backdrop-blur-xl dark:bg-slate-950/[0.10]"
          >
            <WindowControls />
          </div>
        ))}
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
