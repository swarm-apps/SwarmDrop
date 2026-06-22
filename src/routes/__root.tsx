/**
 * Root Layout
 * 应用根布局
 */

import { createRootRoute, Outlet } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { ForceUpdateDialog } from "@/components/force-update-dialog";
import { PromptUpdateDialog } from "@/components/prompt-update-dialog";
import { UpdateProvider } from "@/components/update-provider";
import { useUpdate } from "@/hooks/use-update";
// import { TanStackRouterDevtools } from "@tanstack/router-devtools";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout() {
  // SwarmHive 更新 engine（registry-web）：tauriAdapter 复用 tauri.conf 的 updater
  // endpoint（已切到自托管 SwarmHive 动态端点），挂载即 check、窗口聚焦再 check。
  return (
    <UpdateProvider>
      <UpdateGate />
    </UpdateProvider>
  );
}

function UpdateGate() {
  const { status } = useUpdate();
  const [promptOpen, setPromptOpen] = useState(false);
  const prevStatusRef = useRef(status);

  // 仅当 status 从其他状态变为 "available" 时打开提示弹窗（强更走 ForceUpdateDialog 自管）。
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
