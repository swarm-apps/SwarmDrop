"use client";

/**
 * 发送准备阶段（一遍流式读产出校验和 + 验签树）的进度行。
 *
 * 自己从 store 读活跃批次，不收 props——因为它有**两个**挂载点：发送面板与收件箱
 * 转发对话框。两者走的是同一个 `send_files()`，此前只有前者显示进度，转发大文件时
 * 用户只看得到一个转不完的圈。
 *
 * **进度条走 `tone="local"`（灰）而不是传输条的品牌青绿**：prepare 是纯本机阶段，
 * 还没上网。两者此前同色同形且背靠背出现，大文件上用户会把 prepare 读成传输进度、
 * 再把随后从 0 起的传输条读成「续传倒回 0%」。文案带百分比是同一件事的另一半——
 * 三端同此（桌面 `send/-components/prepare-progress-bar.tsx`、移动端同名文件）。
 */

import { Trans, useLingui } from "@lingui/react/macro";

import { calcPercent, formatFileSize } from "@swarmdrop/shared-view";

import { useWebNode } from "../_lib/store";
import { ProgressBar } from "./progress-bar";

export function PrepareProgressRow() {
  const { t } = useLingui();
  const progress = useWebNode((s) => s.activePrepare);

  if (!progress) return null;

  const percent = calcPercent(progress.bytesHashed, progress.totalBytes);

  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs text-muted-foreground">
        <Trans>
          正在准备 {percent}%（{progress.completedFiles}/{progress.totalFiles} 文件 ·{" "}
          {formatFileSize(progress.bytesHashed)}/{formatFileSize(progress.totalBytes)}）
        </Trans>
      </p>
      <ProgressBar percent={percent} tone="local" label={t`准备文件的进度`} />
      {/* 文件名单独一行并截断：它此前嵌在句子中间，长名字会把百分比与计数挤出可视区。 */}
      {progress.currentFile && (
        <p className="truncate text-xs text-muted-foreground">{progress.currentFile}</p>
      )}
    </div>
  );
}
