/**
 * PrepareProgressBar
 * 发送准备阶段（一遍流式读产出校验和 + 验签树）进度条。发送页（index）与 share-target 共用。
 *
 * 文案说「正在准备」而不是「正在计算校验和」：用户不关心校验和是什么，只关心还要等多久。
 * 三端同一句（移动端 `components/transfer/prepare-progress-bar.tsx`、Web 端
 * `_components/prepare-progress.tsx`）。
 *
 * **它必须与传输条视觉可区分：灰 = 本机在准备、还没上网；品牌青绿 = 数据真的在传。**
 * 两者背靠背出现（prepare 走完 → 跳详情页 → 又一条从 0 起的进度条），大文件上同色同形
 * 足以让用户把 prepare 读成传输、再把新发送读成「续传倒回 0%」——用户实测就是这么误判的。
 * 文案带百分比同理：一条不报数的条子只能靠颜色区分，颜色又正是被记混的那一维。
 * 判据的事实源是 DESIGN.md 的 `Transfer Progress Contract`（三端同一份，机制各不相同）。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import type { PrepareProgress } from "@/lib/types";
import { calcPercent, formatFileSize } from "@/lib/format";
import { Progress } from "@/components/ui/progress";

export function PrepareProgressBar({ progress }: { progress: PrepareProgress }) {
  const { t } = useLingui();
  // 走共享的 calcPercent 而不是手算：它把上限夹到 100。宿主报大或多文件累计有偏差时，
  // 手算会把 >100 的值喂给 <Progress>，条子直接冲出容器。三端同一个函数。
  const percent = calcPercent(progress.bytesHashed, progress.totalBytes);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span className="truncate">
          <Trans>
            正在准备 {percent}%（{progress.completedFiles}/{progress.totalFiles}）
          </Trans>
        </span>
        <span className="ml-2 shrink-0 font-mono tabular-nums">
          {formatFileSize(progress.bytesHashed)} / {formatFileSize(progress.totalBytes)}
        </span>
      </div>
      {/* 阶段用具名 `tone` 而不是在这里手写颜色类：轨道与填充必须成对换（默认轨道带
          `bg-primary/20`，只改填充会变成「灰填充 + 品牌色底槽」），这一对由 <Progress>
          的查表收口，加第三种阶段时才是编译错误而不是漏改的颜色。

          `aria-label` 不是可选装饰：Radix 的 Root 渲染出的是 `role="progressbar"`，
          没有可访问名时读屏只能念出一个孤零零的进度条——而灰/青绿这层区分**完全**靠颜色，
          读屏用户拿到的就是两条一模一样的条子。Web 那份同样传了名字（同一句文案）。 */}
      <Progress
        value={percent}
        tone="local"
        className="h-2"
        aria-label={t`准备文件的进度`}
      />
      {progress.currentFile && (
        <p className="truncate text-xs text-muted-foreground">{progress.currentFile}</p>
      )}
    </div>
  );
}
