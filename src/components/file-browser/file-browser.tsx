import { LayoutGrid, ListTree } from "lucide-react";
import { Trans, useLingui } from "@lingui/react/macro";
import { cn } from "@/lib/utils";
import { formatFileSize } from "@/lib/format";
import { FileGridView } from "./file-grid-view";
import { FileTreeView } from "./file-tree-view";
import type {
  FileBrowserActions,
  FileBrowserEmptyState,
  FileBrowserItem,
  FileBrowserView,
} from "./types";

interface FileBrowserProps {
  items: FileBrowserItem[];
  title?: React.ReactNode;
  view: FileBrowserView;
  availableViews?: readonly FileBrowserView[];
  onViewChange?: (view: FileBrowserView) => void;
  actions?: FileBrowserActions;
  emptyState?: FileBrowserEmptyState;
  className?: string;
  contentClassName?: string;
  testId?: string;
  gridTestId?: string;
  cardTestId?: string;
}

const DEFAULT_VIEWS = ["tree", "grid"] as const;
const TREE_ONLY_VIEW = ["tree"] as const;

export function FileBrowser({
  items,
  title = <Trans>文件</Trans>,
  view,
  availableViews = DEFAULT_VIEWS,
  onViewChange,
  actions,
  emptyState,
  className,
  contentClassName,
  testId = "file-browser",
  gridTestId,
  cardTestId,
}: FileBrowserProps) {
  const { t } = useLingui();
  const safeViews = availableViews.length > 0 ? availableViews : TREE_ONLY_VIEW;
  const resolvedView = safeViews.includes(view) ? view : safeViews[0];
  const totalSize = items.reduce((sum, item) => sum + item.size, 0);
  const showToggle = items.length > 0 && safeViews.length > 1;

  return (
    <section
      data-testid={testId}
      className={cn("flex min-h-0 flex-1 flex-col gap-2.5", className)}
    >
      <header className="flex min-h-8 items-center justify-between gap-3">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold text-foreground">{title}</h3>
          {items.length > 0 && (
            <p className="mt-0.5 text-xs tabular-nums text-muted-foreground">
              <Trans>{items.length} 项 · {formatFileSize(totalSize)}</Trans>
            </p>
          )}
        </div>
        {showToggle && (
          <div
            role="group"
            aria-label={t`文件视图`}
            className="flex shrink-0 items-center rounded-lg bg-foreground/[0.045] p-0.5"
          >
            <button
              type="button"
              data-testid="file-browser-tree-toggle"
              aria-label={t`树形视图`}
              aria-pressed={resolvedView === "tree"}
              onClick={() => onViewChange?.("tree")}
              className={cn(
                "inline-flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                resolvedView === "tree" && "bg-background text-foreground shadow-sm",
              )}
            >
              <ListTree className="size-3.5" />
            </button>
            <button
              type="button"
              data-testid="file-browser-grid-toggle"
              aria-label={t`网格视图`}
              aria-pressed={resolvedView === "grid"}
              onClick={() => onViewChange?.("grid")}
              className={cn(
                "inline-flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                resolvedView === "grid" && "bg-background text-foreground shadow-sm",
              )}
            >
              <LayoutGrid className="size-3.5" />
            </button>
          </div>
        )}
      </header>

      {items.length === 0 ? (
        <div className="flex min-h-32 flex-1 flex-col items-center justify-center rounded-[14px] border border-dashed border-border/70 px-6 text-center">
          <p className="text-sm font-medium text-foreground">
            {emptyState?.title ?? <Trans>暂无文件</Trans>}
          </p>
          {emptyState?.description && (
            <p className="mt-1 max-w-sm text-xs text-muted-foreground">
              {emptyState.description}
            </p>
          )}
        </div>
      ) : (
        <div className={cn("flex min-h-0 flex-1 flex-col", contentClassName)}>
          {resolvedView === "tree" ? (
            <FileTreeView key="tree" items={items} actions={actions} />
          ) : (
            <FileGridView
              key="grid"
              items={items}
              actions={actions}
              testId={gridTestId}
              cardTestId={cardTestId}
            />
          )}
        </div>
      )}
    </section>
  );
}
