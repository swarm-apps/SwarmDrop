import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { FileCard } from "./file-card";
import type { FileBrowserActions, FileBrowserItem } from "./types";

const CARD_MIN_WIDTH = 168;
const GRID_GAP = 12;

export function calculateGridColumns(width: number): number {
  if (width <= 0) return 1;
  return Math.max(1, Math.min(6, Math.floor((width + GRID_GAP) / (CARD_MIN_WIDTH + GRID_GAP))));
}

export function FileGridView({
  items,
  actions,
  cardTestId,
  testId = "file-browser-grid",
}: {
  items: FileBrowserItem[];
  actions?: FileBrowserActions;
  cardTestId?: string;
  testId?: string;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const columns = calculateGridColumns(width);
  const rowCount = Math.ceil(items.length / columns);
  const estimatedRowHeight = useMemo(() => {
    const cardWidth = width > 0
      ? (width - GRID_GAP * (columns - 1) - 12) / columns
      : CARD_MIN_WIDTH;
    return Math.max(190, cardWidth * 0.75 + 74) + GRID_GAP;
  }, [columns, width]);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => estimatedRowHeight,
    overscan: 3,
  });

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    const updateWidth = () => setWidth(element.clientWidth);
    updateWidth();
    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useLayoutEffect(() => {
    virtualizer.measure();
  }, [estimatedRowHeight, virtualizer]);

  return (
    <div
      ref={scrollRef}
      data-testid={testId}
      className="min-h-0 flex-1 overflow-auto rounded-[14px] bg-foreground/[0.018] p-1.5 dark:bg-white/[0.025]"
    >
      <div
        className="relative w-full"
        style={{ height: `${virtualizer.getTotalSize()}px` }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const start = virtualRow.index * columns;
          const rowItems = items.slice(start, start + columns);
          return (
            <div
              key={virtualRow.key}
              data-index={virtualRow.index}
              className="absolute left-0 top-0 grid w-full gap-3 pb-3"
              style={{
                gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              {rowItems.map((item) => (
                <FileCard
                  key={item.id}
                  item={item}
                  actions={actions}
                  testId={cardTestId}
                />
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
