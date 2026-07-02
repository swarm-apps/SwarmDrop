/**
 * OfflineEmptyState
 * 桌面端节点离线时的空状态提示
 */

import { WifiOff, Play } from "lucide-react";
import { Trans } from "@lingui/react/macro";

interface OfflineEmptyStateProps {
  onStartClick: () => void;
}

export function OfflineEmptyState({ onStartClick }: OfflineEmptyStateProps) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4">
      <div className="flex size-20 items-center justify-center rounded-full bg-muted">
        <WifiOff className="size-9 text-muted-foreground" />
      </div>
      <h2 className="text-lg font-semibold text-foreground">
        <Trans>节点未启动</Trans>
      </h2>
      <p className="text-center text-sm text-muted-foreground">
        <Trans>启动 P2P 节点后即可发现附近设备、接收文件并发起配对</Trans>
      </p>
      <button
        type="button"
        onClick={onStartClick}
        className="flex items-center gap-2 rounded-xl bg-primary px-8 py-3 text-base font-semibold text-primary-foreground transition-colors hover:bg-primary/90"
      >
        <Play className="size-[18px]" />
        <Trans>启动节点</Trans>
      </button>
    </div>
  );
}
