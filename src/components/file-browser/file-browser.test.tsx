import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { FileBrowser } from "./file-browser";
import { calculateGridColumns } from "./file-grid-view";
import { FileGridView } from "./file-grid-view";

const items = [{
  id: "one",
  name: "one.txt",
  relativePath: "one.txt",
  size: 10,
  status: "idle" as const,
}];

function renderBrowser(node: React.ReactNode) {
  return render(<I18nProvider i18n={i18n}>{node}</I18nProvider>);
}

afterEach(cleanup);

describe("FileBrowser", () => {
  it("exposes accessible pressed state and emits view changes", async () => {
    const user = userEvent.setup();
    const onViewChange = vi.fn();
    renderBrowser(<FileBrowser items={items} view="tree" onViewChange={onViewChange} />);
    expect(screen.getByRole("button", { name: "树形视图" }).getAttribute("aria-pressed")).toBe("true");
    await user.click(screen.getByRole("button", { name: "网格视图" }));
    expect(onViewChange).toHaveBeenCalledWith("grid");
  });

  it("falls back to an available view and hides the toggle", () => {
    renderBrowser(<FileBrowser items={items} view="grid" availableViews={["tree"]} />);
    expect(screen.getByTestId("file-browser-tree")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "网格视图" })).toBeNull();
  });

  it("renders an empty state without a view toggle", () => {
    renderBrowser(<FileBrowser items={[]} view="grid" emptyState={{ title: "没有内容" }} />);
    expect(screen.getByText("没有内容")).toBeTruthy();
    expect(screen.queryByRole("group", { name: "文件视图" })).toBeNull();
  });

  it("uses a fresh independent scroll area after switching views", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [view, setView] = useState<"tree" | "grid">("tree");
      return <FileBrowser items={items} view={view} onViewChange={setView} />;
    }
    renderBrowser(<Harness />);
    const tree = screen.getByTestId("file-browser-tree");
    tree.scrollTop = 120;
    await user.click(screen.getByTestId("file-browser-grid-toggle"));
    const grid = screen.getByTestId("file-browser-grid");
    expect(grid.scrollTop).toBe(0);
    expect(grid.className).toContain("overflow-auto");
  });
});

describe("grid columns", () => {
  it("responds to container width and caps columns", () => {
    expect(calculateGridColumns(160)).toBe(1);
    expect(calculateGridColumns(360)).toBe(2);
    expect(calculateGridColumns(2000)).toBe(6);
  });

  it("mounts only visible rows for a large collection", async () => {
    const originalResizeObserver = window.ResizeObserver;
    const width = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(720);
    const height = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(480);
    const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      width: 720,
      height: 480,
      top: 0,
      right: 720,
      bottom: 480,
      left: 0,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    class MeasuredResizeObserver {
      constructor(private callback: ResizeObserverCallback) {}
      observe(target: Element) {
        if (target.hasAttribute("data-index")) return;
        queueMicrotask(() => {
          this.callback([{
            target,
            contentRect: target.getBoundingClientRect(),
            borderBoxSize: [{ inlineSize: 720, blockSize: 480 }],
            contentBoxSize: [{ inlineSize: 720, blockSize: 480 }],
            devicePixelContentBoxSize: [],
          } as unknown as ResizeObserverEntry], this);
        });
      }
      unobserve() {}
      disconnect() {}
    }
    window.ResizeObserver = MeasuredResizeObserver as unknown as typeof ResizeObserver;
    const manyItems = Array.from({ length: 10_000 }, (_, index) => ({
      id: String(index),
      name: `file-${index}.txt`,
      relativePath: `folder/file-${index}.txt`,
      size: index,
      status: "idle" as const,
    }));

    renderBrowser(<FileGridView items={manyItems} />);
    await waitFor(() => {
      const mounted = screen.getAllByTestId("file-browser-card").length;
      expect(mounted).toBeGreaterThan(0);
      expect(mounted).toBeLessThan(100);
    });

    window.ResizeObserver = originalResizeObserver;
    width.mockRestore();
    height.mockRestore();
    rect.mockRestore();
  });
});
