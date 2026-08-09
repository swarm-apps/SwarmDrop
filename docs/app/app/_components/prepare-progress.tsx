"use client";

/**
 * 发送准备阶段（一遍流式读产出校验和 + 验签树）的进度行。
 *
 * 自己从 store 读活跃批次，不收 props——因为它有**两个**挂载点：发送面板与收件箱
 * 转发对话框。两者走的是同一个 `send_files()`，此前只有前者显示进度，转发大文件时
 * 用户只看得到一个转不完的圈。
 */

import { Trans, useLingui } from "@lingui/react/macro";

import { calcPercent, formatFileSize } from "@swarmdrop/shared-view";

import { useWebNode } from "../_lib/store";
import { ProgressBar } from "./progress-bar";

export function PrepareProgressRow() {
  const { t } = useLingui();
  const progress = useWebNode((s) => s.activePrepare);

  if (!progress) return null;

  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs text-muted-foreground">
        <Trans>
          正在准备 {progress.currentFile}（{progress.completedFiles}/
          {progress.totalFiles} 文件 · {formatFileSize(progress.bytesHashed)}/
          {formatFileSize(progress.totalBytes)}）
        </Trans>
      </p>
      <ProgressBar
        percent={calcPercent(progress.bytesHashed, progress.totalBytes)}
        label={t`准备文件的进度`}
      />
    </div>
  );
}
