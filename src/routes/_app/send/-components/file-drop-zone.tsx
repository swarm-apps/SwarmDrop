/**
 * FileDropZone
 * 文件拖放区 —— 拖拽文件/文件夹,或通过按钮选择
 */

import { useCallback, useState } from "react";
import { CloudUpload } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { cn } from "@/lib/utils";
import { pickFiles, pickFolderAsSource } from "@/lib/file-picker";
import type { FileSource } from "@/commands/transfer";

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
        "flex flex-col items-center justify-center gap-3.5 rounded-xl border-[1.5px] transition-colors",
        isDragging
          ? "border-blue-500 bg-blue-100"
          : "border-blue-200 bg-[#EFF6FF]",
        disabled && "pointer-events-none opacity-50",
      )}
      style={{ height: 164 }}
    >
      <div className="flex size-12 items-center justify-center rounded-full bg-blue-100">
        <CloudUpload className="size-5.5 text-blue-600" />
      </div>

      <span className="text-[13px] text-muted-foreground">
        <Trans>拖拽文件/文件夹到这里</Trans>
      </span>

      <div className="flex items-center gap-2.5">
        <button
          type="button"
          onClick={handleSelectFiles}
          disabled={disabled}
          className="rounded-lg bg-blue-600 px-5 py-2 text-[13px] font-medium text-white hover:bg-blue-700 disabled:opacity-50"
        >
          <Trans>选择文件</Trans>
        </button>
        <button
          type="button"
          onClick={handleSelectFolder}
          disabled={disabled}
          className="rounded-lg border-[1.5px] border-blue-500 px-5 py-2 text-[13px] font-medium text-blue-500 hover:bg-blue-50 disabled:opacity-50"
        >
          <Trans>选择文件夹</Trans>
        </button>
      </div>
    </div>
  );
}
