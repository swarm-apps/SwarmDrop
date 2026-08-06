/**
 * `@swarmdrop/file-browser` 的唯一公开面。
 *
 * 桌面（`src/`）与 Web（`docs/app/app`）都从这里导入，不深链子路径——子模块划分是本包的
 * 内部事。移动端不消费本包（React Native 与 React DOM 不共用 JSX），它共用的是
 * `@swarmdrop/shared-view` 里的纯逻辑。
 *
 * 什么进本包、什么进 shared-view、什么留各端，见 README 的「归属判据」。
 *
 * ## 公开面刻意窄
 *
 * 只有 `FileBrowser` 与它的入参类型，外加两端**确实**要用的 `getFileIconStyle`
 * （收件箱条目的类型图标）。此前把 `FileGridView` / `FileTreeView` / `FileCard` / `FileRow` /
 * `toHeadlessTreeData` / `TREE_ROOT_ID` / `cn` 等 20 来个符号全导了出去——外部一个消费者
 * 都没有，却等于对外承诺了「树库不会换、卡片可以单独复用」这些本包并不打算承诺的事。
 * 包内互相引用走相对路径，**测试也直接 import 子模块**（`use-thumbnail.test.tsx` 就是）。
 *
 * **纯逻辑与展示模型不在这里重导出**：`FileBrowserItem`、四个 adapter、`buildFileBrowserTree`
 * 等一律从 `@swarmdrop/shared-view` 取。给它们开第二条入口会让「这个类型到底归谁」变得含糊，
 * 而那正是移动端要不要跟着改的判据。
 */

export { FileBrowser } from "./file-browser";
export { getFileIconStyle } from "./file-icon";

export type {
  FileBrowserActions,
  FileBrowserEmptyState,
  FileBrowserTarget,
  ThumbnailResolver,
} from "./types";
