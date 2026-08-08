/**
 * useNodeRestart
 * 「改了设置 → 需重启节点生效」的共用逻辑：
 * - markRestartNeeded(): 改动后调用，仅在节点运行时标记需要重启
 * - restart(): 停再起，成功清除标记并提示，失败保留标记供重试
 * - showBanner: 是否展示重启提示条（需要重启 且 节点仍在运行）
 * - activeTransferCount: 重启会断开的在途传输数，供调用方把后果说出来
 *
 * **标记住在 `network-store` 而不是本 hook 的 `useState`。** 它此前是组件局部状态，
 * 于是「改完开关 → 切去设备页 → 回设置页」提示就没了，用户以为已经生效。设置区有两处
 * （网络 / 引导节点）各自持有一份也是同一个病：改了 A 区、B 区不知道。
 */

import { useCallback, useState } from "react";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { toast } from "sonner";
import { useActiveTransferCount } from "@/hooks/use-active-transfer-count";
import { useNetworkStore } from "@/stores/network-store";

export function useNodeRestart() {
  const { t } = useLingui();
  const nodeStatus = useNetworkStore((s) => s.status);
  const stopNetwork = useNetworkStore((s) => s.stopNetwork);
  const startNetwork = useNetworkStore((s) => s.startNetwork);
  const needsRestart = useNetworkStore((s) => s.needsRestart);
  const setNeedsRestart = useNetworkStore((s) => s.setNeedsRestart);
  // 重启 = 停再起，在途传输会当场断掉。这个数暴露给调用方，让「重启节点」那颗
  // 按钮旁边能说清后果——此前这条路径对在途传输零提示、零防护。
  const activeTransferCount = useActiveTransferCount();
  // 「正在重启」是这次点击的局部事实，不是全局状态：两个设置区同时挂着这个 hook 时，
  // 转圈只该出现在被点的那一颗按钮上。
  const [restarting, setRestarting] = useState(false);

  const markRestartNeeded = useCallback(() => {
    if (nodeStatus === "running") {
      setNeedsRestart(true);
    }
  }, [nodeStatus, setNeedsRestart]);

  const restart = useCallback(async () => {
    setRestarting(true);
    try {
      await stopNetwork();
      const ok = await startNetwork();
      if (!ok) {
        // startNetwork 失败时内部已 toast 原因；保留 needsRestart 供重试，
        // 不显示成功提示（避免把启动失败掩盖成「已重启」）。
        setNeedsRestart(true);
        return;
      }
      // startNetwork 成功时已把标记清零（它读的就是当前偏好）。
      toast.success(t(msg`节点已重启`));
    } catch {
      toast.error(t(msg`重启节点失败`));
    } finally {
      setRestarting(false);
    }
  }, [startNetwork, stopNetwork, setNeedsRestart, t]);

  return {
    restarting,
    markRestartNeeded,
    restart,
    activeTransferCount,
    showBanner: needsRestart && nodeStatus === "running",
  };
}
