/**
 * App Layout
 * 需要认证的页面布局（带侧边栏）
 * - desktop (≥1024px): 侧边栏展开
 * - tablet (768-1023px): 侧边栏折叠为图标模式
 * - mobile (<768px): 隐藏侧边栏，显示底部导航栏
 */

import { createFileRoute, Link, Outlet, useLocation } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useEffect } from "react";
import { AppSidebar } from "@/components/layout/sidebar";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { BottomNav } from "@/components/layout/bottom-nav";
import { useNetworkStore } from "@/stores/network-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useBreakpoint } from "@/hooks/use-breakpoint";
import { ConnectionRequestDialog } from "@/components/pairing/connection-request-dialog";
import { TransferOfferDialog } from "@/components/transfer/transfer-offer-dialog";
import {
  setupTransferListeners,
  cleanupTransferListeners,
} from "@/stores/transfer-store";

export const Route = createFileRoute("/_app")({
  component: AppLayout,
});

function AppLayout() {
  const breakpoint = useBreakpoint();
  const isMobile = breakpoint === "mobile";
  const isDesktop = breakpoint === "desktop";

  // 传输事件监听
  useEffect(() => {
    setupTransferListeners();
    return () => {
      cleanupTransferListeners();
    };
  }, []);

  // 自动启动节点（解锁后首次进入时检查）
  useEffect(() => {
    const { autoStart } = usePreferencesStore.getState();
    const { status, startNetwork } = useNetworkStore.getState();
    if (autoStart && status === "stopped") {
      startNetwork();
    }
  }, []);

  const location = useLocation();

  // send/receive/pairing 页面为独立全屏，不显示全局 header 和 bottom nav
  const isFullScreenRoute =
    location.pathname.startsWith("/send") ||
    location.pathname.startsWith("/receive") ||
    location.pathname.startsWith("/pairing");

  // 移动端：只有首页（设备页）显示顶部标题栏
  const isHomePage = location.pathname === "/" || location.pathname === "/devices";

  if (isMobile) {
    return (
      <div className="flex h-svh flex-col pt-[env(safe-area-inset-top)]">
        {isHomePage && <MobileHeader />}
        <main className="flex-1 overflow-hidden">
          <Outlet />
        </main>
        {!isFullScreenRoute && <BottomNav />}
        <ConnectionRequestDialog />
        <TransferOfferDialog />
      </div>
    );
  }

  return (
    <SidebarProvider
      className="h-svh"
      defaultOpen={isDesktop}
      style={
        {
          "--sidebar-width": "220px",
        } as React.CSSProperties
      }
    >
      <AppSidebar />
      <SidebarInset className="overflow-hidden">
        <Outlet />
      </SidebarInset>
      <ConnectionRequestDialog />
      <TransferOfferDialog />
    </SidebarProvider>
  );
}

function MobileHeader() {
  return (
    <header className="flex items-center justify-between px-5 py-2">
      <span className="text-2xl font-bold text-foreground">SwarmDrop</span>
      <Link
        to="/pairing"
        className="flex size-9 items-center justify-center rounded-full bg-blue-600 text-white transition-colors hover:bg-blue-700"
      >
        <Plus className="size-5" />
      </Link>
    </header>
  );
}
