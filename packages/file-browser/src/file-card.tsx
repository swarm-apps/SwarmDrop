import { memo, useState } from "react";
import { Check, CircleAlert, ImageOff, Timer } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { Progress } from "./progress";
import { cn } from "./cn";
import { formatFileSize } from "@swarmdrop/shared-view";
import { getFileIconStyle } from "./file-icon";
import { FileItemActions } from "./item-actions";
import { useThumbnail } from "./use-thumbnail";
import { getParentPath } from "@swarmdrop/shared-view";
import type { FileBrowserItem } from "@swarmdrop/shared-view";
import type { FileBrowserActions, ThumbnailResolver } from "./types";

function FileCardComponent({
  item,
  actions,
  thumbnailSource,
}: {
  item: FileBrowserItem;
  actions?: FileBrowserActions;
  thumbnailSource?: ThumbnailResolver;
}) {
  const [failedPreviewUrl, setFailedPreviewUrl] = useState<string>();
  const { icon: Icon, color: iconColor } = getFileIconStyle(item.name);
  const directory = getParentPath(item.relativePath);
  const progress = Math.round(item.progress ?? 0);
  const canOpen = Boolean(actions?.onOpen) && item.status !== "missing";
  const { ref: previewRef, url: thumbnailUrl } = useThumbnail(item, thumbnailSource);
  /**
   * 两条取图路径**由字段本身区分**，组件不推断自己跑在哪一端：
   *
   * - `previewUrl`：已经能直接渲染（桌面的 asset URL）。
   * - `previewSource`：要经 resolver + 缩放管线（Web 的 OPFS 路径），产物是上面的 `thumbnailUrl`。
   *
   * 这里曾写作 `thumbnailSource ? thumbnailUrl : item.previewSource`——把「调用方有没有传
   * 取图源函数」当成「我在哪一端」的代理变量，而类型系统一个字都表达不出那条不变量。
   */
  const previewUrl = item.previewUrl ?? thumbnailUrl;
  const showPreview = Boolean(previewUrl)
    && previewUrl !== failedPreviewUrl
    && item.status !== "missing";

  const preview = (
    <div
      ref={previewRef}
      className="relative aspect-[4/3] overflow-hidden bg-foreground/[0.035] dark:bg-white/[0.04]"
    >
      {showPreview ? (
        <img
          src={previewUrl}
          alt=""
          loading="lazy"
          className="size-full object-cover"
          onError={() => setFailedPreviewUrl(previewUrl)}
        />
      ) : (
        <div className="flex size-full items-center justify-center">
          {item.status === "missing" ? (
            <ImageOff className="size-10 text-muted-foreground/55" />
          ) : (
            <Icon className={cn("size-11", iconColor)} />
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
      {/* 悬停才露出的动作条。`pointer-coarse:opacity-100`：触摸屏上没有 hover，第一次点
          只是把按钮显出来、第二次才真的触发——而 Web 端是移动优先的，那等于把动作藏起来。 */}
      <div className="absolute right-2 top-2 z-20 rounded-lg border border-white/40 bg-background/75 p-0.5 opacity-0 shadow-sm backdrop-blur-md transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 dark:border-white/10">
        <FileItemActions item={item} actions={actions} />
      </div>
      {item.status === "transferring" && (
        <div className="absolute inset-x-2 bottom-2 rounded-md bg-background/80 p-2 backdrop-blur-md">
          <div className="mb-1 flex justify-between text-[10px] font-medium tabular-nums">
            <Trans>传输中</Trans>
            <span>{progress}%</span>
          </div>
          {/* 上面那行已经写着「传输中 X%」，进度条在这里是同一信息的第二次表达——
              所以是装饰性的（`label={null}`），不再挂一个 progressbar 角色。 */}
          <Progress value={progress} className="h-1" label={null} />
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
      data-testid="file-browser-card"
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

/** 同 `FileRow`：默认浅比较对每秒重建的 `item` 永远判不等，必须自定义比较器。 */
export const FileCard = memo(FileCardComponent, (prev, next) => {
  const a = prev.item;
  const b = next.item;
  return (
    prev.actions === next.actions &&
    prev.thumbnailSource === next.thumbnailSource &&
    a.id === b.id &&
    a.status === b.status &&
    a.progress === b.progress &&
    a.name === b.name &&
    a.size === b.size &&
    a.previewUrl === b.previewUrl &&
    a.previewSource === b.previewSource
  );
});
