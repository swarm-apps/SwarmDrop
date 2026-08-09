/**
 * PrepareProgressBar
 * 发送准备阶段（一遍流式读产出校验和 + 验签树）进度条。发送页（index）与 share-target 共用。
 *
 * 文案说「正在准备」而不是「正在计算校验和」：用户不关心校验和是什么，只关心还要等多久。
 * 三端同一句（移动端 `components/transfer/prepare-progress-bar.tsx`、Web 端
 * `_components/prepare-progress.tsx`）。
 */

import { Trans } from "@lingui/react/macro";
import type { PrepareProgress } from "@/lib/types";
import { calcPercent, formatFileSize } from "@/lib/format";
import { Progress } from "@/components/ui/progress";

export function PrepareProgressBar({ progress }: { progress: PrepareProgress }) {
  // 走共享的 calcPercent 而不是手算：它把上限夹到 100。宿主报大或多文件累计有偏差时，
  // 手算会把 >100 的值喂给 <Progress>，条子直接冲出容器。三端同一个函数。
  const percent = calcPercent(progress.bytesHashed, progress.totalBytes);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span className="truncate">
          <Trans>
            正在准备 ({progress.completedFiles}/{progress.totalFiles})
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
