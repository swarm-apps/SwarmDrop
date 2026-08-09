import { memo } from "react";
import { Check, CircleAlert, Pause, Timer, XCircle } from "lucide-react";
import { Trans, useLingui } from "@lingui/react/macro";
import { Progress } from "./progress";
import { cn } from "./cn";
import { formatFileSize } from "@swarmdrop/shared-view";
import { getFileIconStyle } from "./file-icon";
import { FileItemActions } from "./item-actions";
import type { FileBrowserItem } from "@swarmdrop/shared-view";
import type { FileBrowserActions } from "./types";

const statusStyles: Record<FileBrowserItem["status"], string> = {
  idle: "hover:bg-foreground/[0.045]",
  waiting: "text-muted-foreground hover:bg-foreground/[0.035]",
  transferring: "bg-primary/[0.07] ring-1 ring-inset ring-primary/20",
  // 暂停与传输中同属「进行时」，但没有活动感——所以保留环、去掉底色的强调。
  paused: "ring-1 ring-inset ring-primary/15 hover:bg-foreground/[0.035]",
  completed: "hover:bg-emerald-500/[0.055]",
  // 取消不是错误（没传成，但没出错），所以走静音而不是 destructive 的红环。
  cancelled: "text-muted-foreground hover:bg-foreground/[0.035]",
  error: "bg-destructive/[0.055] ring-1 ring-inset ring-destructive/15",
  missing: "text-muted-foreground opacity-70 hover:bg-foreground/[0.035]",
};

function FileRowComponent({
  item,
  level,
  actions,
}: {
  item: FileBrowserItem;
  level: number;
  actions?: FileBrowserActions;
}) {
  const { t } = useLingui();
  const { icon: Icon, color: iconColor } = getFileIconStyle(item.name);
  const progress = Math.round(item.progress ?? 0);
  // 传输中与暂停都该看得见进度条——暂停时用户最关心的正是「停在了哪」。
  const showProgress = item.status === "transferring" || item.status === "paused";

  return (
    <div
      className={cn(
        "group flex min-h-10 items-center gap-2 rounded-lg pr-2 transition-colors focus-within:ring-2 focus-within:ring-inset focus-within:ring-ring/55",
        statusStyles[item.status],
      )}
      style={{ paddingLeft: `${level * 22 + 8}px` }}
    >
      <Icon className={cn("size-[18px] shrink-0", iconColor)} />
      <div className="min-w-0 flex-1 py-1.5">
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-sm text-foreground">
            {item.name}
          </span>
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
            {showProgress ? `${progress}%` : formatFileSize(item.size)}
          </span>
          {item.status === "waiting" && <Timer className="size-3.5 text-muted-foreground" />}
          {item.status === "paused" && <Pause className="size-3.5 text-muted-foreground" />}
          {item.status === "completed" && <Check className="size-3.5 text-emerald-500" />}
          {item.status === "cancelled" && <XCircle className="size-3.5 text-muted-foreground" />}
          {item.status === "error" && <CircleAlert className="size-3.5 text-destructive" />}
          {item.status === "missing" && (
            <span className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
              <Trans>缺失</Trans>
            </span>
          )}
        </div>
        {showProgress && (
          <Progress
            value={progress}
            className="mt-1 h-1"
            label={t`${item.name} 的进度`}
          />
        )}
      </div>
      {/* 「常驻还是 hover 才露出」由 `ActionBar` 自己判（见 `item-actions.tsx`）。 */}
      <FileItemActions item={item} actions={actions} />
    </div>
  );
}

/**
 * **必须 memo，且必须自定义比较器。**
 *
 * 传输进行中，`itemsFromProjection` 每秒重建全部 item 对象，默认浅比较对 `item` 永远判不等
 * ——加了 `memo` 也等于没加。而一次 200 文件的会话里通常只有一个文件在动，其余可见行每秒
 * 白重渲染一次：每行要跑 `useLingui()`、一次分类查表、六七次 `cn()`（clsx + tailwind-merge
 * 的字符串解析），外加动作条里最多四个按钮各自再 `cn()` 一次。
 *
 * 比较的是**真正会变的四个字段**。名称/大小/路径在一次会话里恒定，但仍列进来——
 * 收件箱与发送侧会原地替换条目，漏掉它们就会显示上一个文件的名字。
 */
export const FileRow = memo(FileRowComponent, (prev, next) => {
  const a = prev.item;
  const b = next.item;
  return (
    prev.level === next.level &&
    prev.actions === next.actions &&
    a.id === b.id &&
    a.status === b.status &&
    a.progress === b.progress &&
    a.name === b.name &&
    a.size === b.size
  );
});
