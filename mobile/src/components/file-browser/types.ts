/**
 * 文件浏览器的类型面。
 *
 * **展示模型已上移到 `@swarmdrop/shared-view`**（三端共用一份 `FileBrowserItem`）。
 * 这里只作转发 + 声明移动端**表现层特有**的三样：动作集合、组件 props、列表容器语境。
 * 前者的类型里带 `ReactElement`，后者是 FlashList / BottomSheetFlatList 的选择——
 * 都进不了那个平台中立的包。
 *
 * 上移带来的两处改名，改动波及本目录所有组件：
 *
 * - `size: bigint` → `size: number`。uniffi 给的是 `bigint`，在 `adapters.ts` 那一层转。
 *   文件大小碰不到 `Number.MAX_SAFE_INTEGER`（9 PB）。
 * - `localUri` → `previewSource`。三端语义不同（移动是 `file://`、桌面是资源协议 URL、
 *   Web 是 OPFS 相对路径），所以取了个不预设形态的名字。
 */

import type {
  FileBrowserItem,
  FileBrowserScope,
  FileBrowserView,
} from "@swarmdrop/shared-view";
import type { ReactElement } from "react";

export type {
  FileBrowserDirectoryNode,
  FileBrowserFileNode,
  FileBrowserItem,
  FileBrowserScope,
  FileBrowserStatus,
  FileBrowserTree,
  FileBrowserTreeNode,
  FileBrowserTreeRow,
  FileBrowserView,
} from "@swarmdrop/shared-view";

/** 列表挂在普通页面还是 bottom sheet 里——决定用 FlashList 还是 BottomSheetFlatList。 */
export type FileBrowserListContext = "screen" | "bottom-sheet";

export interface FileBrowserActions {
  removeItem?: (item: FileBrowserItem) => void;
  removeDirectory?: (relativeDirectory: string) => void;
  openItem?: (item: FileBrowserItem) => void;
  shareItem?: (item: FileBrowserItem) => void;
  /** 转发到另一台设备（收件箱专用）。 */
  sendItem?: (item: FileBrowserItem) => void;
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
