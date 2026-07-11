import { Check, CircleAlert, Timer } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import { formatFileSize } from "@/lib/format";
import { getFileIcon, getFileIconColor } from "@/lib/file-icon";
import { FileItemActions } from "./item-actions";
import type { FileBrowserActions, FileBrowserItem } from "./types";

const statusStyles: Record<FileBrowserItem["status"], string> = {
  idle: "hover:bg-foreground/[0.045]",
  waiting: "text-muted-foreground hover:bg-foreground/[0.035]",
  transferring: "bg-primary/[0.07] ring-1 ring-inset ring-primary/20",
  completed: "hover:bg-emerald-500/[0.055]",
  error: "bg-destructive/[0.055] ring-1 ring-inset ring-destructive/15",
  missing: "text-muted-foreground opacity-70 hover:bg-foreground/[0.035]",
};

export function FileRow({
  item,
  level,
  actions,
}: {
  item: FileBrowserItem;
  level: number;
  actions?: FileBrowserActions;
}) {
  const Icon = getFileIcon(item.name);
  const progress = Math.round(item.progress ?? 0);

  return (
    <div
      className={cn(
        "group flex min-h-10 items-center gap-2 rounded-lg pr-2 transition-colors focus-within:ring-2 focus-within:ring-inset focus-within:ring-ring/55",
        statusStyles[item.status],
      )}
      style={{ paddingLeft: `${level * 22 + 8}px` }}
    >
      <Icon className={cn("size-[18px] shrink-0", getFileIconColor(item.name))} />
      <div className="min-w-0 flex-1 py-1.5">
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-sm text-foreground">
            {item.name}
          </span>
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
            {item.status === "transferring" ? `${progress}%` : formatFileSize(item.size)}
          </span>
          {item.status === "waiting" && <Timer className="size-3.5 text-muted-foreground" />}
          {item.status === "completed" && <Check className="size-3.5 text-emerald-500" />}
          {item.status === "error" && <CircleAlert className="size-3.5 text-destructive" />}
          {item.status === "missing" && (
            <span className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
              <Trans>缺失</Trans>
            </span>
          )}
        </div>
        {item.status === "transferring" && (
          <Progress value={progress} className="mt-1 h-1" />
        )}
      </div>
      <FileItemActions
        item={item}
        actions={actions}
        className="opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
      />
    </div>
  );
}
