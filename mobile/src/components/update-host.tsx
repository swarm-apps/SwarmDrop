/**
 * 更新 UI 入口（仅 Android）—— 接 registry-rn / SwarmHive 更新引擎。
 *
 * 组合 registry-rn 三个弹窗：
 *   - PromptUpdateDialog（受控）：status 变 "available" 时弹，含内联下载进度。
 *   - ForceUpdateDialog（自管）：status === "force-required" 时弹，不可关闭。
 *   - UpdateProgressDialog（自管）：downloading / ready 常驻进度（prompt 关闭后接管）。
 *
 * iOS 直接返回 null，完整 iOS 升级路径走 TestFlight / App Store。
 * 必须挂在 <UpdateProvider> 内（useUpdate 依赖其 context）。
 */
import { t } from "@lingui/core/macro";
import { useEffect, useRef, useState } from "react";
import { Platform } from "react-native";
import { ForceUpdateDialog } from "@/components/force-update-dialog";
import { PromptUpdateDialog } from "@/components/prompt-update-dialog";
import { UpdateProgressDialog } from "@/components/update-progress-dialog";
import { useUpdate } from "@/hooks/use-update";
import { toast } from "@/lib/toast";

export function UpdateHost() {
  if (Platform.OS !== "android") return null;
  return <AndroidUpdateHost />;
}

function AndroidUpdateHost() {
  const { status, error } = useUpdate();
  const [promptOpen, setPromptOpen] = useState(false);
  const prevStatusRef = useRef(status);
  const lastErrorRef = useRef<unknown>(null);

  useEffect(() => {
    // status 从其他态变为 "available" → 弹 prompt（强更走 ForceUpdateDialog 自管）。
    if (prevStatusRef.current !== "available" && status === "available") {
      setPromptOpen(true);
    }
    // 进入下载/强更后收起 prompt，交接给 UpdateProgressDialog / ForceUpdateDialog（顺序 UX）。
    if (status === "downloading" || status === "force-required") {
      setPromptOpen(false);
    }
    prevStatusRef.current = status;
  }, [status]);

  useEffect(() => {
    if (status !== "error" || !error || lastErrorRef.current === error) {
      return;
    }
    lastErrorRef.current = error;
    toast.error(t`更新失败`, error);
  }, [status, error]);

  return (
    <>
      <PromptUpdateDialog open={promptOpen} onOpenChange={setPromptOpen} />
      <ForceUpdateDialog />
      <UpdateProgressDialog />
    </>
  );
}
