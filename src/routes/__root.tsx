/**
 * Root Layout
 * 应用根布局
 */

import { createRootRoute, Outlet } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import {
  ForceUpdateDialog,
  PromptUpdateDialog,
} from "@/components/upgrade";
import { useUpgradeLinkStore } from "@/stores/upgrade-link-store";
// import { TanStackRouterDevtools } from "@tanstack/router-devtools";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout() {
  const status = useUpgradeLinkStore((s) => s.status);
  const checkForUpdate = useUpgradeLinkStore((s) => s.checkForUpdate);
  const [promptOpen, setPromptOpen] = useState(false);
  const prevStatusRef = useRef(status);

  // 应用启动时检查更新
  useEffect(() => {
    // 延迟检查，避免影响启动速度
    const timer = setTimeout(() => {
      checkForUpdate();
    }, 3000);
    return () => clearTimeout(timer);
  }, [checkForUpdate]);

  // 仅当 status 从其他状态变为 "available" 时打开提示弹窗
  useEffect(() => {
    if (prevStatusRef.current !== "available" && status === "available") {
      setPromptOpen(true);
    }
    prevStatusRef.current = status;
  }, [status]);

  return (
    <>
      <Outlet />
      <ForceUpdateDialog />
      <PromptUpdateDialog open={promptOpen} onOpenChange={setPromptOpen} />
      {/* {import.meta.env.DEV && <TanStackRouterDevtools />} */}
    </>
  );
}
