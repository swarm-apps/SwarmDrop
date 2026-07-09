/**
 * PrepareProgressBar
 * 发送准备阶段（BLAKE3 校验和计算）进度条。发送页（index）与 share-target 共用。
 */

import { Trans } from "@lingui/react/macro";
import type { PrepareProgress } from "@/lib/types";
import { formatFileSize } from "@/lib/format";
import { Progress } from "@/components/ui/progress";

export function PrepareProgressBar({ progress }: { progress: PrepareProgress }) {
  const percent =
    progress.totalBytes > 0
      ? Math.round((progress.bytesHashed / progress.totalBytes) * 100)
      : 0;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span className="truncate">
          <Trans>
            正在计算校验和 ({progress.completedFiles}/{progress.totalFiles})
          </Trans>
        </span>
        <span className="ml-2 shrink-0 font-mono tabular-nums">
          {formatFileSize(progress.bytesHashed)} / {formatFileSize(progress.totalBytes)}
        </span>
      </div>
      <Progress value={percent} className="h-2" />
      {progress.currentFile && (
        <p className="truncate text-xs text-muted-foreground">{progress.currentFile}</p>
      )}
    </div>
  );
}
