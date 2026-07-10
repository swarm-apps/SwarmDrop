import type { ReactElement } from "react";

export type FileBrowserStatus =
  | "idle"
  | "waiting"
  | "transferring"
  | "paused"
  | "completed"
  | "cancelled"
  | "error"
  | "missing";

export type FileBrowserView = "tree" | "grid";
export type FileBrowserScope = "send" | "transfer" | "inbox";
export type FileBrowserListContext = "screen" | "bottom-sheet";

export interface FileBrowserItem {
  id: string;
  fileId?: number;
  sourceId?: string;
  name: string;
  relativePath: string;
  size: bigint;
  status: FileBrowserStatus;
  progress?: number;
  previewUri?: string;
}

export interface FileBrowserActions {
  removeItem?: (item: FileBrowserItem) => void;
  removeDirectory?: (relativeDirectory: string) => void;
  openItem?: (item: FileBrowserItem) => void;
  shareItem?: (item: FileBrowserItem) => void;
  revealItem?: (item: FileBrowserItem) => void;
  retryItem?: (item: FileBrowserItem) => void;
}

export interface FileBrowserProps {
  items: FileBrowserItem[];
  scope: FileBrowserScope;
  actions?: FileBrowserActions;
  title?: ReactElement | string;
  contentHeader?: ReactElement | null;
  contentFooter?: ReactElement | null;
  listContext?: FileBrowserListContext;
  testID?: string;
  resetKey?: string;
  initialScrollIndex?: number;
  onViewChange?: (view: FileBrowserView) => void;
}
