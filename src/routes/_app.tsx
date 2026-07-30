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
import { ClipboardInviteBanner } from "@/components/pairing/clipboard-invite-banner";
import {
  setupTransferListeners,
  cleanupTransferListeners,
} from "@/stores/transfer-store";

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

  // 剪贴板提示的落点就是粘贴页，所以在那一页上它是纯噪音（页面本身就是那个入口）。
  // 其余页面一律亮，包括 /pairing/generate —— 在展示自己邀请的同时收到别人的邀请，
  // 是个合理场景。
  const isInviteInputRoute = location.pathname.startsWith("/pairing/input");

  return (
    <div className="app-shell flex h-svh flex-col">
      <AppAmbientBackground />
      {/* 顶栏在**所有**路由上挂，配对页也不例外。它此前是全屏路由、自绘一条只有窗口
          按钮的玻璃条：那条 strip 与页面自己的 TaskToolbar 叠成两层头，且 Windows 上
          看起来像没做完。AppTopBar 本身就带 data-tauri-drag-region 与 WindowControls
          （macOS 靠 pl-20 给红绿灯留位），无边框窗口的拖拽/控制它全包了，特例是多余的。
          /send 一直是「AppTopBar + TaskToolbar」两层，配对页照此对齐。 */}
      <AppTopBar />
      {/* 剪贴板里有别人给的邀请时抬一条非模态提示（自己发出的不亮，见组件文档）。
          放在 main 之上、顶栏之下：它是入站信号，不该被页面内容挤走。 */}
      {!isInviteInputRoute && <ClipboardInviteBanner />}
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
