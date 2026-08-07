import { useMemo } from "react";
import { LayoutGrid, ListTree } from "lucide-react";
import { Trans, useLingui } from "@lingui/react/macro";
import { cn } from "./cn";
import { formatFileSize } from "@swarmdrop/shared-view";
import { FileGridView } from "./file-grid-view";
import { FileTreeView } from "./file-tree-view";
import type { FileBrowserItem, FileBrowserView } from "@swarmdrop/shared-view";
import type {
  FileBrowserActions,
  FileBrowserEmptyState,
  ThumbnailResolver,
} from "./types";

interface FileBrowserProps {
  items: FileBrowserItem[];
  title?: React.ReactNode;
  view: FileBrowserView;
  onViewChange?: (view: FileBrowserView) => void;
  actions?: FileBrowserActions;
  /**
   * 作用于**整个集合**的动作（如「全部下载」），渲染在表头、视图切换按钮左边。
   *
   * `FileBrowserActions` 里的每一项都绑定在单个条目上，集合级动作在那套模型里没有位置；
   * 而它天然属于表头——「6 项 · 3.9 MB」这行说的正是这个集合。放页脚则要先滚过整份清单
   * 才看得见，多文件条目上尤其别扭（那恰恰是最需要它的场合）。
   */
  headerActions?: React.ReactNode;
  /**
   * 网格缩略图的取图源。**不传即不生成缩略图**，卡片画类型图标。
   * 桌面不传（它的 `previewSource` 已经是可直接渲染的 asset URL），Web 传 OPFS 取图。
   */
  thumbnailSource?: ThumbnailResolver;
  emptyState?: FileBrowserEmptyState;
  className?: string;
  contentClassName?: string;
}

export function FileBrowser({
  items,
  title = <Trans>文件</Trans>,
  view,
  onViewChange,
  actions,
  headerActions,
  thumbnailSource,
  emptyState,
  className,
  contentClassName,
}: FileBrowserProps) {
  const { t } = useLingui();
  // 表头的总计随 items 变（传输中每秒一次），但重算是 O(n) 加法——memo 掉，避免视图切换
  // 按钮的 hover 之类的无关重渲染也去扫一遍几百个文件。
  const totalSize = useMemo(
    () => items.reduce((sum, item) => sum + item.size, 0),
    [items],
  );
  const showToggle = items.length > 0;

  return (
    <section
      data-testid="file-browser"
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
        {/* 空态下两者都不在，整块不渲染——留一个空 div 会白占 header 的 `gap-3`。 */}
        {(headerActions || showToggle) && (
          <div className="flex shrink-0 items-center gap-2">
            {headerActions}
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
                  aria-pressed={view === "tree"}
                  onClick={() => onViewChange?.("tree")}
                  className={cn(
                    "inline-flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    view === "tree" && "bg-background text-foreground shadow-sm",
                  )}
                >
                  <ListTree className="size-3.5" />
                </button>
                <button
                  type="button"
                  data-testid="file-browser-grid-toggle"
                  aria-label={t`网格视图`}
                  aria-pressed={view === "grid"}
                  onClick={() => onViewChange?.("grid")}
                  className={cn(
                    "inline-flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    view === "grid" && "bg-background text-foreground shadow-sm",
                  )}
                >
                  <LayoutGrid className="size-3.5" />
                </button>
              </div>
            )}
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
          {view === "tree" ? (
            <FileTreeView items={items} actions={actions} />
          ) : (
            <FileGridView
              items={items}
              actions={actions}
              thumbnailSource={thumbnailSource}
            />
          )}
        </div>
      )}
    </section>
  );
}
