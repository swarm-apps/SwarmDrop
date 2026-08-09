import { memo } from "react";
import { ChevronRight, Folder, FolderOpen } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { cn } from "./cn";
import { formatFileSize } from "@swarmdrop/shared-view";
import { FolderItemActions } from "./item-actions";
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
  /**
   * 整份动作集，不是单独的 `onRemove`——目录行现在也有取回动作（Web 端下载整棵子树），
   * 而它要读 `pendingIds`。逐项摊平会让每加一个目录动作就改一次本组件的 props。
   */
  actions?: FileBrowserActions;
}

function FolderRowComponent({
  node,
  level,
  expanded,
  onToggle,
  actions,
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
        // **只认「整行本身」被聚焦的那一下。** 动作条里的按钮也在这个 div 内，不判来源
        // 的话：键盘用户 Tab 到目录的下载按钮、按 Enter → 事件冒泡到这里 →
        // `preventDefault()` 把浏览器据以生成 click 的默认行为掐掉 → **按钮的 onClick
        // 从来不会跑**，只有目录折叠了一下。而树形视图里那颗按钮是取回整个目录的唯一入口。
        // `stopPropagation` 挡不住这个：它是冒泡阶段的同一条链，动作条挡的是 click 不是 keydown。
        if (event.target !== event.currentTarget) return;
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
      {/* 「常驻还是 hover 才露出」由 `ActionBar` 自己判——它才知道这条动作条实际渲染
          得出哪些按钮（见 `item-actions.tsx`）。 */}
      <FolderItemActions relativePath={node.relativePath} actions={actions} />
    </div>
  );
}

/**
 * 同 `FileRow`。`node` 每次建树都是新对象，所以比较的是它的字段；`onToggle` 由调用方
 * 保持稳定（见 `file-tree-view.tsx` 的 `toggle`）。
 *
 * `actions` 按引用比：调用方本就要求它稳定（否则 `FileRow` 也一起被打穿），而 `pendingIds`
 * 变化时那个对象会换引用——目录行的 spinner 正是靠这一条跟上的。
 */
export const FolderRow = memo(FolderRowComponent, (prev, next) => {
  const a = prev.node;
  const b = next.node;
  return (
    prev.level === next.level &&
    prev.expanded === next.expanded &&
    prev.onToggle === next.onToggle &&
    prev.actions === next.actions &&
    a.id === b.id &&
    a.relativePath === b.relativePath &&
    a.name === b.name &&
    a.fileCount === b.fileCount &&
    a.size === b.size
  );
});
