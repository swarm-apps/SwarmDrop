import { useState } from "react";
import { Check, CircleAlert, ImageOff, Timer } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import { formatFileSize } from "@/lib/format";
import { getFileIcon, getFileIconColor } from "@/lib/file-icon";
import { FileItemActions } from "./item-actions";
import { getParentPath } from "./tree-data";
import type { FileBrowserActions, FileBrowserItem } from "./types";

export function FileCard({
  item,
  actions,
  testId = "file-browser-card",
}: {
  item: FileBrowserItem;
  actions?: FileBrowserActions;
  testId?: string;
}) {
  const [failedPreviewUrl, setFailedPreviewUrl] = useState<string>();
  const Icon = getFileIcon(item.name);
  const directory = getParentPath(item.relativePath);
  const progress = Math.round(item.progress ?? 0);
  const canOpen = Boolean(actions?.onOpen) && item.status !== "missing";
  const showPreview = Boolean(item.previewUrl)
    && item.previewUrl !== failedPreviewUrl
    && item.status !== "missing";

  const preview = (
    <div className="relative aspect-[4/3] overflow-hidden bg-foreground/[0.035] dark:bg-white/[0.04]">
      {showPreview ? (
        <img
          src={item.previewUrl}
          alt=""
          loading="lazy"
          className="size-full object-cover"
          onError={() => setFailedPreviewUrl(item.previewUrl)}
        />
      ) : (
        <div className="flex size-full items-center justify-center">
          {item.status === "missing" ? (
            <ImageOff className="size-10 text-muted-foreground/55" />
          ) : (
            <Icon className={cn("size-11", getFileIconColor(item.name))} />
          )}
        </div>
      )}
      {canOpen && (
        <button
          type="button"
          aria-label={item.name}
          className="absolute inset-0 z-10 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60"
          onClick={() => actions?.onOpen?.(item)}
        />
      )}
      <div className="absolute right-2 top-2 z-20 rounded-lg border border-white/40 bg-background/75 p-0.5 opacity-0 shadow-sm backdrop-blur-md transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 dark:border-white/10">
        <FileItemActions item={item} actions={actions} />
      </div>
      {item.status === "transferring" && (
        <div className="absolute inset-x-2 bottom-2 rounded-md bg-background/80 p-2 backdrop-blur-md">
          <div className="mb-1 flex justify-between text-[10px] font-medium tabular-nums">
            <Trans>传输中</Trans>
            <span>{progress}%</span>
          </div>
          <Progress value={progress} className="h-1" />
        </div>
      )}
      {item.status === "missing" && (
        <span className="absolute left-2 top-2 rounded-full bg-background/85 px-2 py-1 text-[10px] font-medium text-muted-foreground backdrop-blur-md">
          <Trans>文件缺失</Trans>
        </span>
      )}
    </div>
  );

  return (
    <article
      data-testid={testId}
      className={cn(
        "group min-w-0 overflow-hidden rounded-[14px] border border-border/55 bg-background/44 shadow-[0_8px_24px_rgba(15,23,42,0.045)] transition-colors",
        "hover:border-primary/25 focus-within:border-primary/35",
        item.status === "error" && "border-destructive/25",
        item.status === "missing" && "opacity-75",
      )}
    >
      {preview}
      <div className="space-y-1 px-3 py-2.5">
        <div className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground" title={item.name}>
            {item.name}
          </span>
          {item.status === "waiting" && <Timer className="size-3.5 text-muted-foreground" />}
          {item.status === "completed" && <Check className="size-3.5 text-emerald-500" />}
          {item.status === "error" && <CircleAlert className="size-3.5 text-destructive" />}
        </div>
        <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
          <span className="min-w-0 truncate" title={directory}>
            {directory || <Trans>根目录</Trans>}
          </span>
          <span className="shrink-0 tabular-nums">{formatFileSize(item.size)}</span>
        </div>
      </div>
    </article>
  );
}
