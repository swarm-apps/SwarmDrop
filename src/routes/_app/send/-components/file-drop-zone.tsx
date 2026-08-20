/**
 * FileDropZone
 * 文件拖放区 —— 拖拽文件/文件夹,或通过按钮选择
 */

import { CloudUpload } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { cn } from "@/lib/utils";
import { pickFiles, pickFolderAsSource } from "@/lib/file-picker";
import type { FileSource } from "@/lib/bindings";

interface FileDropZoneProps {
  onSourcesSelected: (sources: FileSource[]) => void;
  disabled?: boolean;
  /**
   * 是否有文件正拖在窗口上方。
   *
   * **拖放订阅归页面所有，这里只画高亮**——`hasFiles` 翻转会让这个组件 remount，
   * 而触发翻转的正是投放本身，订阅留在这儿会在那一刻退订重订。见 send/index.lazy.tsx。
   */
  isDragging?: boolean;
  /** 已有文件时收成补充入口，空态则作为主操作区展开。 */
  compact?: boolean;
  className?: string;
}

export function FileDropZone({
  onSourcesSelected,
  disabled,
  isDragging = false,
  compact = false,
  className,
}: FileDropZoneProps) {
  const handleSelectFiles = async () => {
    const sources = await pickFiles(true);
    if (sources.length > 0) {
      onSourcesSelected(sources);
    }
  };

  const handleSelectFolder = async () => {
    const source = await pickFolderAsSource();
    if (source) {
      onSourcesSelected([source]);
    }
  };

  return (
    <div
      data-testid="file-drop-zone"
      className={cn(
        "group flex rounded-[20px] border border-dashed p-4 transition-[background-color,border-color,transform,box-shadow] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)]",
        compact
          ? "min-h-[96px] flex-row items-center justify-between gap-4"
          : "min-h-[260px] flex-col items-center justify-center gap-5 px-6",
        isDragging
          ? "border-primary/50 bg-primary/10 shadow-[0_18px_46px_rgb(8_121_104_/_0.12)]"
          : "border-brand/20 bg-white/[0.24] hover:border-brand/40 hover:bg-primary/[0.045] dark:bg-white/[0.025] dark:hover:bg-primary/[0.08]",
        disabled && "pointer-events-none opacity-50",
        className,
      )}
    >
      <div
        className={cn(
          "flex min-w-0 items-center",
          compact ? "gap-3" : "flex-col gap-3 text-center",
        )}
      >
        <div className="glass-control flex size-11 shrink-0 items-center justify-center rounded-[16px] text-brand transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:-translate-y-0.5">
          <CloudUpload className="size-5" />
        </div>
        <span className="text-[13px] text-muted-foreground">
          <Trans>拖拽文件或文件夹到这里</Trans>
        </span>
      </div>

      <div className="flex shrink-0 items-center gap-2.5">
        <button
          type="button"
          onClick={handleSelectFiles}
          disabled={disabled}
          data-testid="select-files-action"
          className="rounded-full bg-primary px-5 py-2 text-[13px] font-medium text-primary-foreground shadow-[0_10px_22px_rgb(8_121_104_/_0.18)] transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-primary/90 active:scale-[0.98] disabled:opacity-50"
        >
          <Trans>选择文件</Trans>
        </button>
        <button
          type="button"
          onClick={handleSelectFolder}
          disabled={disabled}
          data-testid="select-folder-action"
          className="glass-control rounded-full px-5 py-2 text-[13px] font-medium text-brand transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/55 active:scale-[0.98] disabled:opacity-50 dark:hover:bg-white/[0.07]"
        >
          <Trans>选择文件夹</Trans>
        </button>
      </div>
    </div>
  );
}
