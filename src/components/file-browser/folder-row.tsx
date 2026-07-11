import { ChevronRight, Folder, FolderOpen } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { cn } from "@/lib/utils";
import { formatFileSize } from "@/lib/format";
import { RemoveAction } from "./item-actions";
import type { FileBrowserActions, FileBrowserTreeNode } from "./types";

interface FolderRowProps {
  node: FileBrowserTreeNode;
  level: number;
  expanded: boolean;
  onToggle: () => void;
  onRemove?: FileBrowserActions["onRemove"];
}

export function FolderRow({
  node,
  level,
  expanded,
  onToggle,
  onRemove,
}: FolderRowProps) {
  const FolderIcon = expanded ? FolderOpen : Folder;

  return (
    <div
      role="button"
      tabIndex={0}
      aria-expanded={expanded}
      className={cn(
        "group flex h-10 cursor-pointer items-center gap-2 rounded-lg pr-2 text-foreground",
        "transition-colors hover:bg-foreground/[0.045] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/55",
      )}
      style={{ paddingLeft: `${level * 22 + 8}px` }}
      onClick={onToggle}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onToggle();
        }
      }}
    >
      <ChevronRight
        className={cn(
          "size-3.5 shrink-0 text-muted-foreground transition-transform duration-150",
          expanded && "rotate-90",
        )}
      />
      <FolderIcon className="size-[18px] shrink-0 text-amber-500/90" />
      <span className="min-w-0 flex-1 truncate text-sm font-medium">
        {node.name}
      </span>
      <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
        <Trans>{node.fileCount ?? 0} 项</Trans>
        {node.size > 0 && ` · ${formatFileSize(node.size)}`}
      </span>
      <div
        className="opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
        onClick={(event) => event.stopPropagation()}
      >
        <RemoveAction
          target={{ type: "directory", relativePath: node.relativePath }}
          onRemove={onRemove}
        />
      </div>
    </div>
  );
}
