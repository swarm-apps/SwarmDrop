/**
 * useNodeRestart
 * 抽取网络 / 引导节点设置共用的「改了设置 → 需重启节点生效」逻辑：
 * - markRestartNeeded(): 改动后调用，仅在节点运行时标记需要重启
 * - restart(): 停再起，成功清除标记并提示，失败保留标记供重试
 * - showBanner: 是否展示重启提示条（需要重启 且 节点仍在运行）
 * - activeTransferCount: 重启会断开的在途传输数，供调用方把后果说出来
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
  // 重启 = 停再起，在途传输会当场断掉。这个数暴露给调用方，让「重启节点」那颗
  // 按钮旁边能说清后果——此前这条路径对在途传输零提示、零防护。
  const activeTransferCount = useActiveTransferCount();
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
    activeTransferCount,
    showBanner: needsRestart && nodeStatus === "running",
  };
}
