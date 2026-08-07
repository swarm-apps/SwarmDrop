/**
 * 建树 —— **已上移到 `@swarmdrop/shared-view`**，本模块只作转发。
 *
 * 上移的是算法本身（按 `relativePath` 派生目录层级、目录递归累计 size/fileCount、
 * 目录优先排序），产出中立的**嵌套树**；`flattenVisibleNodes` 把它按展开态拍平成
 * FlashList 要的行，也一并共用——桌面那边则把同一棵树投影成 `@headless-tree` 的
 * `Map` + `dataLoader`，两种消费形态互不迁就。
 *
 * 共享版比原来多两样：`FileBrowserTree` 带 `totalSize` / `totalCount`（此前是各页
 * 自己再 reduce 一遍），行的 `size` 是 `number`（此前 `bigint`）。
 */

export type {
  FileBrowserDirectoryNode,
  FileBrowserFileNode,
  FileBrowserTree,
  FileBrowserTreeNode,
  FileBrowserTreeRow,
} from "@swarmdrop/shared-view";
export {
  buildFileBrowserTree,
  flattenVisibleNodes,
} from "@swarmdrop/shared-view";
