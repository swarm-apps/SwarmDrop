import { Download, FolderOpen, LoaderCircle, LocateFixed, RotateCcw, Send, X } from "lucide-react";
import { useLingui } from "@lingui/react/macro";
import { cn } from "./cn";
import type { FileBrowserItem } from "@swarmdrop/shared-view";
import type { FileBrowserActions, FileBrowserTarget } from "./types";

interface ActionButtonProps {
  label: string;
  onClick?: () => void;
  destructive?: boolean;
  disabled?: boolean;
  children: React.ReactNode;
}

function ActionButton({
  label,
  onClick,
  destructive,
  disabled,
  children,
}: ActionButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onClick?.();
      }}
      className={cn(
        "inline-flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors",
        "hover:bg-foreground/[0.06] hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        "disabled:pointer-events-none disabled:opacity-35",
        destructive && "hover:bg-destructive/10 hover:text-destructive",
      )}
    >
      {children}
    </button>
  );
}

export function RemoveAction({
  target,
  onRemove,
}: {
  target: FileBrowserTarget;
  onRemove?: FileBrowserActions["onRemove"];
}) {
  const { t } = useLingui();
  if (!onRemove) return null;
  return (
    <ActionButton label={t`移除`} destructive onClick={() => onRemove(target)}>
      <X className="size-3.5" />
    </ActionButton>
  );
}

/**
 * 这组动作里有没有**主动作**——有的话动作条常驻，而不是 hover 才露出。
 *
 * 主动作 = 把文件从沙盒里弄出去的那条路。浏览器端有两条：`onDownload` 取回本机，
 * `onSend` 送去另一台设备；那一端没有「打开文件」也没有「在文件夹中显示」，藏起这两个
 * 就等于藏起整个出口，而收件箱存在的理由正是「把收到的东西弄走」。桌面端的动作全是
 * 便利项（文件早就落盘了），继续 hover 才露出。
 *
 * 判据是「这组动作里有没有主动作」，**不是「跑在哪一端」**——组件仍然不认识平台，
 * 与 `FileBrowserActions` 那条「不传就不渲染」的并集约定同一个路子。
 */
export function hasPrimaryAction(actions?: FileBrowserActions): boolean {
  return Boolean(actions?.onDownload || actions?.onSend);
}

export function FileItemActions({
  item,
  actions,
  className,
}: {
  item: FileBrowserItem;
  actions?: FileBrowserActions;
  className?: string;
}) {
  const { t } = useLingui();
  const isMissing = item.status === "missing";
  const isPending = actions?.pendingIds?.has(item.id) ?? false;
  if (!actions) return null;

  return (
    <div
      className={cn("flex items-center gap-0.5", className)}
      onClick={(event) => event.stopPropagation()}
    >
      {actions.onDownload && (
        <ActionButton
          label={isPending ? t`正在准备下载` : t`下载`}
          disabled={isMissing || isPending}
          onClick={() => actions.onDownload?.(item)}
        >
          {isPending ? (
            <LoaderCircle className="size-3.5 animate-spin" />
          ) : (
            <Download className="size-3.5" />
          )}
        </ActionButton>
      )}
      {actions.onSend && (
        <ActionButton
          label={t`发送到设备`}
          disabled={isMissing}
          onClick={() => actions.onSend?.(item)}
        >
          <Send className="size-3.5" />
        </ActionButton>
      )}
      {actions.onOpen && (
        <ActionButton
          label={t`打开文件`}
          disabled={isMissing}
          onClick={() => actions.onOpen?.(item)}
        >
          <FolderOpen className="size-3.5" />
        </ActionButton>
      )}
      {actions.onReveal && (
        <ActionButton
          label={t`在文件夹中显示`}
          disabled={isMissing}
          onClick={() => actions.onReveal?.(item)}
        >
          <LocateFixed className="size-3.5" />
        </ActionButton>
      )}
      {actions.onRetry && item.status === "error" && (
        <ActionButton label={t`重试`} onClick={() => actions.onRetry?.(item)}>
          <RotateCcw className="size-3.5" />
        </ActionButton>
      )}
      {actions.onRemove && (
        <RemoveAction
          target={{ type: "file", item }}
          onRemove={actions.onRemove}
        />
      )}
    </div>
  );
}
