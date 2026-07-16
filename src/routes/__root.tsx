/**
 * Root Layout
 * 应用根布局
 */

import { createRootRoute, Outlet } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { CloseBehaviorManager } from "@/components/close-behavior-manager";
import { ForceUpdateDialog } from "@/components/force-update-dialog";
import { PromptUpdateDialog } from "@/components/prompt-update-dialog";
import { UpdateProgressDialog } from "@/components/update-progress-dialog";
import { UpdateProvider } from "@/components/update-provider";
import { useUpdate } from "@/hooks/use-update";
import { progressDialogVisible } from "@/lib/update-dialog-visibility";
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

/**
 * 三个弹窗的可见性必须互斥：两个 Dialog 同框时，上层那个 modal overlay 会吃掉下层
 * release notes 的滚动与点击，并把下层压暗。互斥判据集中在 @/lib/update-dialog-visibility
 * （上游 registry 有全组合不变量测试），此处与各弹窗内部都不要再据 status 自行推导。
 */
function UpdateGate() {
  const { status, release } = useUpdate();
  const [promptOpen, setPromptOpen] = useState(false);
  const prevStatusRef = useRef(status);

  useEffect(() => {
    // 仅当 status 从其他状态变为 "available" 时打开提示弹窗（强更走 ForceUpdateDialog 自管）。
    if (prevStatusRef.current !== "available" && status === "available") {
      setPromptOpen(true);
    }
    // 进入强更后收起 prompt，交接给 ForceUpdateDialog（它不可关，必须独占）。
    // 下载中【不】收：prompt 自带内联进度，保持打开 = 用户在下载期间仍看得到 release notes，
    // 且全程只有一个弹窗。
    if (status === "force-required") {
      setPromptOpen(false);
    }
    prevStatusRef.current = status;
  }, [status]);

  return (
    <>
      <Outlet />
      <CloseBehaviorManager />
      <ForceUpdateDialog />
      <PromptUpdateDialog open={promptOpen} onOpenChange={setPromptOpen} />
      {/* 兜底进度视图：仅当没有别的弹窗在承载进度时才出现——prompt 开着时它自带内联进度
          （故 !promptOpen），强更流由 ForceUpdateDialog 承载（故 progressDialogVisible 判
          upgradeType）。用户在下载中关掉 prompt 后由它接管，否则进度就彻底消失了。 */}
      <UpdateProgressDialog
        open={!promptOpen && progressDialogVisible(status, release)}
      />
      {/* {import.meta.env.DEV && <TanStackRouterDevtools />} */}
    </>
  );
}
