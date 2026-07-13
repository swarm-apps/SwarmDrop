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
  /**
   * 本设备上可访问的 `file://` 路径(存在时)。发送 scope 来自 `sourceId`,
   * 收件箱 scope 来自未缺失文件的 `localPath`;transfer/offer scope 不设。
   * 缩略图管线(`useFileThumbnail`)据此解析图片/视频缩略图。
   */
  localUri?: string;
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
