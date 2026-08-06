import { memo } from "react";
import { ChevronRight, Folder, FolderOpen } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { cn } from "./cn";
import { formatFileSize } from "@swarmdrop/shared-view";
import { RemoveAction } from "./item-actions";
import type { FileBrowserActions } from "./types";

interface FolderRowProps {
  /**
   * 只声明用到的字段，不绑定具体的节点类型——树的节点在 L1 是嵌套形态、在
   * `headless-tree-adapter` 里又多一个虚拟根变体，写死任一种都会逼另一种做断言。
   */
  node: {
    id: string;
    name: string;
    relativePath: string;
    fileCount: number;
    size: number;
  };
  level: number;
  expanded: boolean;
  /**
   * 收 `nodeId` 而不是无参——这样调用方可以传**一个稳定的**回调给所有行，而不是每行现造
   * 一个箭头函数。本组件是 memo 的，每行新引用会把它整片打穿。
   */
  onToggle: (nodeId: string) => void;
  onRemove?: FileBrowserActions["onRemove"];
}

function FolderRowComponent({
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
      onClick={() => onToggle(node.id)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onToggle(node.id);
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
        <Trans>{node.fileCount} 项</Trans>
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

/**
 * 同 `FileRow`。`node` 每次建树都是新对象，所以比较的是它的字段；`onToggle` 由调用方
 * 保持稳定（见 `file-tree-view.tsx` 的 `toggle`）。
 */
export const FolderRow = memo(FolderRowComponent, (prev, next) => {
  const a = prev.node;
  const b = next.node;
  return (
    prev.level === next.level &&
    prev.expanded === next.expanded &&
    prev.onToggle === next.onToggle &&
    prev.onRemove === next.onRemove &&
    a.id === b.id &&
    a.relativePath === b.relativePath &&
    a.name === b.name &&
    a.fileCount === b.fileCount &&
    a.size === b.size
  );
});
