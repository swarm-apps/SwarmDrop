import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FolderRow } from "./folder-row";
import { FileRow } from "./file-row";
import { FileCard } from "./file-card";
import type { FileBrowserItem, FileBrowserTreeNode } from "./types";

afterEach(cleanup);

function renderWithI18n(node: React.ReactNode) {
  return render(<I18nProvider i18n={i18n}>{node}</I18nProvider>);
}

const directory: FileBrowserTreeNode = {
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
      previewUrl: "asset://photo.png",
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
