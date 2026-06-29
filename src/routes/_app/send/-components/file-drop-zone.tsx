/**
 * FileDropZone
 * 文件拖放区 —— 拖拽文件/文件夹,或通过按钮选择
 */

import { useCallback, useState } from "react";
import { CloudUpload } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { cn } from "@/lib/utils";
import { pickFiles, pickFolderAsSource } from "@/lib/file-picker";
import type { FileSource } from "@/lib/bindings";

interface FileDropZoneProps {
  onSourcesSelected: (sources: FileSource[]) => void;
  disabled?: boolean;
}

export function FileDropZone({ onSourcesSelected, disabled }: FileDropZoneProps) {
  const [isDragging, setIsDragging] = useState(false);

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    if (!disabled) setIsDragging(true);
  }

  function handleDragLeave(e: React.DragEvent) {
    e.preventDefault();
    setIsDragging(false);
  }

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragging(false);
      if (disabled) return;

      const sources: FileSource[] = [];
      const items = e.dataTransfer.items;
      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        // Tauri 环境下 File 对象带有 path 属性(非标准 Web API)
        const file = item.getAsFile() as (File & { path?: string }) | null;
        if (file?.path) {
          sources.push({ type: "path", path: file.path });
        }
      }
      if (sources.length > 0) {
        onSourcesSelected(sources);
      }
    },
    [disabled, onSourcesSelected],
  );

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
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={cn(
        "group flex flex-col items-center justify-center gap-3.5 rounded-[24px] p-2 transition-[background-color,transform,box-shadow] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)]",
        isDragging
          ? "glass-accent scale-[0.995] shadow-[0_18px_46px_rgba(37,99,235,0.12)]"
          : "glass-card hover:shadow-[0_16px_42px_rgba(15,23,42,0.06)]",
        disabled && "pointer-events-none opacity-50",
      )}
      style={{ height: 164 }}
    >
      <div className="flex h-full w-full flex-col items-center justify-center gap-3 rounded-[18px] bg-white/30 shadow-[inset_0_1px_0_rgba(255,255,255,0.42)] dark:bg-white/[0.035] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]">
        <div className="glass-control flex size-12 items-center justify-center rounded-[18px] text-blue-600 transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:-translate-y-0.5 dark:text-blue-300">
          <CloudUpload className="size-5.5" />
        </div>

        <span className="text-[13px] text-muted-foreground">
          <Trans>拖拽文件或文件夹到这里</Trans>
        </span>

        <div className="flex items-center gap-2.5">
          <button
            type="button"
            onClick={handleSelectFiles}
            disabled={disabled}
            className="rounded-full bg-blue-600 px-5 py-2 text-[13px] font-medium text-white shadow-[0_10px_22px_rgba(37,99,235,0.18)] transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-blue-700 active:scale-[0.98] disabled:opacity-50"
          >
            <Trans>选择文件</Trans>
          </button>
          <button
            type="button"
            onClick={handleSelectFolder}
            disabled={disabled}
            className="glass-control rounded-full px-5 py-2 text-[13px] font-medium text-blue-600 transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/55 active:scale-[0.98] disabled:opacity-50 dark:text-blue-300 dark:hover:bg-white/[0.07]"
          >
            <Trans>选择文件夹</Trans>
          </button>
        </div>
      </div>
    </div>
  );
}
