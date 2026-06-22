/**
 * 通用 file-tree 组件 —— 树形展示 + 进度展示
 *
 * 接受 [`TreeDataLoader`](data.ts) 作为数据源，与具体来源解耦。
 * 现在被三处复用：
 * - `routes/_app/send/*`：发送端选文件 UI
 * - `routes/_app/transfer/$sessionId.lazy.tsx`：传输详情
 * - `components/transfer/transfer-offer-dialog.tsx`：接收 offer 弹窗
 */

export { FileTree } from "./file-tree";
export { FileTreeItem, RemoveButton } from "./file-tree-item";
export { FolderRow } from "./folder-row";
export {
  buildTreeData,
  buildTreeDataFromOffer,
  buildTreeDataFromSession,
  computeRelativePath,
} from "./data";
export type {
  EntryPoint,
  FileMeta,
  FileStatus,
  TreeData,
  TreeDataLoader,
  TreeNodeData,
} from "./data";
