/**
 * 更新 UI 入口（仅 Android）—— 接 registry-rn / SwarmHive 更新引擎。
 *
 * 组合 registry-rn 三个弹窗：
 *   - PromptUpdateDialog（受控）：status 变 "available" 时弹，自带内联下载进度，下载中保持打开。
 *   - ForceUpdateDialog（自管）：仅强制流（release.upgradeType === "force"）弹，不可关闭。
 *   - UpdateProgressDialog（受控）：兜底——仅当 prompt 被用户关掉、且非强制流时接管进度。
 *
 * 三者的可见性必须互斥：两个弹窗同框时，上层那个全屏 overlay 会吞掉下层 release notes 的
 * 滚动手势，并把下层压暗。互斥判据集中在 @/lib/update-dialog-visibility（上游 registry 有
 * 全组合不变量测试），此处不要再据 status 自行推导。
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
import { progressDialogVisible } from "@/lib/update-dialog-visibility";

export function UpdateHost() {
  if (Platform.OS !== "android") return null;
  return <AndroidUpdateHost />;
}

function AndroidUpdateHost() {
  const { status, release, error } = useUpdate();
  const [promptOpen, setPromptOpen] = useState(false);
  const prevStatusRef = useRef(status);
  const lastErrorRef = useRef<unknown>(null);

  useEffect(() => {
    // status 从其他态变为 "available" → 弹 prompt（强更走 ForceUpdateDialog 自管）。
    if (prevStatusRef.current !== "available" && status === "available") {
      setPromptOpen(true);
    }
    // 进入强更后收起 prompt，交接给 ForceUpdateDialog（它不可关，必须独占）。
    // 下载中【不】收:prompt 自带内联进度,保持打开 = 用户在下载期间仍看得到 release notes,
    // 且全程只有一个弹窗。曾在此收起 prompt 换纯进度弹窗,等于下载一开始就把更新内容抽走。
    if (status === "force-required") {
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
      {/* 兜底进度视图:仅当没有别的弹窗在承载进度时才出现——prompt 开着时它自带内联进度
          (故 !promptOpen),强更流由 ForceUpdateDialog 承载(故 progressDialogVisible 判
          upgradeType)。用户在下载中主动关掉 prompt 后,由它接管,这是它唯一的用武之地。 */}
      <UpdateProgressDialog
        open={!promptOpen && progressDialogVisible(status, release)}
      />
    </>
  );
}
