import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FolderRow } from "./folder-row";
import { FileRow } from "./file-row";
import { FileCard } from "./file-card";
import type { FileBrowserItem } from "@swarmdrop/shared-view";

afterEach(cleanup);

function renderWithI18n(node: React.ReactNode) {
  return render(<I18nProvider i18n={i18n}>{node}</I18nProvider>);
}

const directory = {
  id: "directory:docs/",
  name: "docs",
  type: "directory",
  relativePath: "docs/",
  size: 12,
  fileCount: 2,
};

describe("file tree rows", () => {
  it("does not add a persistent accent background when expanded", () => {
    renderWithI18n(<FolderRow node={directory} level={0} expanded onToggle={() => {}} />);
    const row = screen.getByRole("button", { name: /docs/ });
    expect(row.className).not.toContain("bg-accent");
    expect(row.getAttribute("aria-expanded")).toBe("true");
  });

  it("toggles folders with Enter and Space", () => {
    const onToggle = vi.fn();
    renderWithI18n(<FolderRow node={directory} level={0} expanded={false} onToggle={onToggle} />);
    const row = screen.getByRole("button", { name: /docs/ });
    fireEvent.keyDown(row, { key: "Enter" });
    fireEvent.keyDown(row, { key: " " });
    expect(onToggle).toHaveBeenCalledTimes(2);
  });

  /**
   * 目录取回是一个**独立的动作**，不是「替用户点 N 次文件下载」——那条路在浏览器上走不完
   * （连续多次程序化下载会被拦），也留不住目录层级。所以 target 必须是整个目录。
   */
  it("downloads a folder as one target instead of its files", async () => {
    const user = userEvent.setup();
    const onDownload = vi.fn();
    const onToggle = vi.fn();
    renderWithI18n(
      <FolderRow
        node={directory}
        level={0}
        expanded
        onToggle={onToggle}
        actions={{ onDownload }}
      />,
    );
    await user.click(screen.getByRole("button", { name: "下载" }));
    expect(onDownload).toHaveBeenCalledWith({
      type: "directory",
      relativePath: "docs/",
    });
    // 动作按钮嵌在一整行可点击的目录行里——不挡住冒泡的话，点下载会顺带折叠这个目录。
    expect(onToggle).not.toHaveBeenCalled();
  });

  /**
   * pending 的查询键是**目录自己的相对路径**，也就是 target 里那个值——不是建树时派生的
   * 节点 id。调用方只认识自己发出去和收回来的东西，`dir:` 那套编码不该漏出包外。
   */
  /**
   * 回归：目录行整行是 `role="button"` 且在 Enter/Space 上 `preventDefault()`，不判事件来源
   * 的话会把动作条里按钮的激活一起吞掉——按钮的 onClick 从来不跑，只有目录折叠了一下。
   * **必须用键盘断言**：`user.click` 走的是另一条路径，鼠标下一切正常。
   */
  it("activates folder actions from the keyboard without toggling the row", async () => {
    const user = userEvent.setup();
    const onDownload = vi.fn();
    const onToggle = vi.fn();
    renderWithI18n(
      <FolderRow
        node={directory}
        level={0}
        expanded
        onToggle={onToggle}
        actions={{ onDownload }}
      />,
    );
    screen.getByRole("button", { name: "下载" }).focus();
    await user.keyboard("{Enter}");
    expect(onDownload).toHaveBeenCalledWith({
      type: "directory",
      relativePath: "docs/",
    });
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("marks a folder busy while its archive is being built", () => {
    renderWithI18n(
      <FolderRow
        node={directory}
        level={0}
        expanded
        onToggle={() => {}}
        actions={{
          onDownload: vi.fn(),
          pendingIds: new Set([directory.relativePath]),
        }}
      />,
    );
    const button = screen.getByRole("button", {
      name: "正在准备下载",
    }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it("keeps remove keyboard-accessible without firing a primary action", async () => {
    const user = userEvent.setup();
    const onRemove = vi.fn();
    const item: FileBrowserItem = {
      id: "one",
      name: "one.txt",
      relativePath: "one.txt",
      size: 10,
      status: "error",
    };
    renderWithI18n(<FileRow item={item} level={0} actions={{ onRemove }} />);
    await user.click(screen.getByRole("button", { name: "移除" }));
    expect(onRemove).toHaveBeenCalledWith({ type: "file", item });
  });

  it("shows retry only for failed files", async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    const item: FileBrowserItem = {
      id: "failed",
      fileId: 7,
      name: "failed.bin",
      relativePath: "failed.bin",
      size: 10,
      status: "error",
    };
    renderWithI18n(<FileRow item={item} level={0} actions={{ onRetry }} />);
    await user.click(screen.getByRole("button", { name: "重试" }));
    expect(onRetry).toHaveBeenCalledWith(item);
  });
});

describe("file cards", () => {
  it("isolates secondary actions from the preview primary action", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    const onReveal = vi.fn();
    const item: FileBrowserItem = {
      id: "photo",
      name: "photo.png",
      relativePath: "images/photo.png",
      size: 42,
      previewSource: "asset://photo.png",
      status: "completed",
    };
    renderWithI18n(<FileCard item={item} actions={{ onOpen, onReveal }} />);
    await user.click(screen.getByRole("button", { name: "在文件夹中显示" }));
    expect(onReveal).toHaveBeenCalledWith(item);
    expect(onOpen).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "photo.png" }));
    expect(onOpen).toHaveBeenCalledWith(item);
  });

  it("disables open and reveal operations for missing files", () => {
    const item: FileBrowserItem = {
      id: "missing",
      name: "gone.png",
      relativePath: "gone.png",
      size: 42,
      status: "missing",
    };
    renderWithI18n(<FileCard item={item} actions={{ onOpen: vi.fn(), onReveal: vi.fn() }} />);
    expect(screen.queryByRole("button", { name: "gone.png" })).toBeNull();
    expect((screen.getByRole("button", { name: "在文件夹中显示" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
