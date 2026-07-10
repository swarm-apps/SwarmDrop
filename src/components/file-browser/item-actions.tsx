import { FolderOpen, LocateFixed, RotateCcw, X } from "lucide-react";
import { useLingui } from "@lingui/react/macro";
import { cn } from "@/lib/utils";
import type {
  FileBrowserActions,
  FileBrowserItem,
  FileBrowserTarget,
} from "./types";

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
  if (!actions) return null;

  return (
    <div
      className={cn("flex items-center gap-0.5", className)}
      onClick={(event) => event.stopPropagation()}
    >
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
