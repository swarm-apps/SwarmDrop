import type { ReactNode } from "react";

export type FileBrowserView = "tree" | "grid";

export type FileBrowserScope = "send" | "inbox" | "transfer";

export type FileBrowserStatus =
  | "idle"
  | "waiting"
  | "transferring"
  | "completed"
  | "error"
  | "missing";

/** 统一文件展示模型。previewUrl 必须由调用方按安全边界显式提供。 */
export interface FileBrowserItem {
  id: string;
  /** 调用方原始记录 ID，用于把显式操作映射回业务命令。 */
  sourceId?: string | number;
  fileId?: number;
  name: string;
  relativePath: string;
  size: number;
  localPath?: string;
  previewUrl?: string;
  status: FileBrowserStatus;
  progress?: number;
}

export type FileBrowserTarget =
  | { type: "file"; item: FileBrowserItem }
  | { type: "directory"; relativePath: string };

export interface FileBrowserActions {
  onRemove?: (target: FileBrowserTarget) => void;
  onOpen?: (item: FileBrowserItem) => void;
  onReveal?: (item: FileBrowserItem) => void;
  onRetry?: (item: FileBrowserItem) => void;
}

export interface FileBrowserEmptyState {
  title: ReactNode;
  description?: ReactNode;
}

export interface FileBrowserTreeNode {
  id: string;
  name: string;
  type: "file" | "directory";
  relativePath: string;
  size: number;
  fileCount?: number;
  item?: FileBrowserItem;
}

export interface FileBrowserTreeData {
  nodes: Map<string, FileBrowserTreeNode>;
  children: Map<string, string[]>;
  rootChildren: string[];
  dataLoader: {
    getItem: (itemId: string) => FileBrowserTreeNode;
    getChildren: (itemId: string) => string[];
  };
}
