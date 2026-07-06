/**
 * useNodeRestart
 * 抽取网络 / 引导节点设置共用的「改了设置 → 需重启节点生效」逻辑：
 * - markRestartNeeded(): 改动后调用，仅在节点运行时标记需要重启
 * - restart(): 停再起，成功清除标记并提示，失败保留标记供重试
 * - showBanner: 是否展示重启提示条（需要重启 且 节点仍在运行）
 */

import { useCallback, useState } from "react";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { toast } from "sonner";
import { useNetworkStore } from "@/stores/network-store";

export function useNodeRestart() {
  const { t } = useLingui();
  const nodeStatus = useNetworkStore((s) => s.status);
  const stopNetwork = useNetworkStore((s) => s.stopNetwork);
  const startNetwork = useNetworkStore((s) => s.startNetwork);
  const [needsRestart, setNeedsRestart] = useState(false);
  const [restarting, setRestarting] = useState(false);

  const markRestartNeeded = useCallback(() => {
    if (nodeStatus === "running") {
      setNeedsRestart(true);
    }
  }, [nodeStatus]);

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
      setNeedsRestart(false);
      toast.success(t(msg`节点已重启`));
    } catch {
      toast.error(t(msg`重启节点失败`));
    } finally {
      setRestarting(false);
    }
  }, [startNetwork, stopNetwork, t]);

  return {
    restarting,
    markRestartNeeded,
    restart,
    showBanner: needsRestart && nodeStatus === "running",
  };
}
