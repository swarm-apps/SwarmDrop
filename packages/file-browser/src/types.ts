/**
 * L2 特有的类型。
 *
 * 展示模型（`FileBrowserItem` / `FileBrowserStatus` / `FileBrowserView` /
 * `FileBrowserScope` / 树节点）全部住在 L1（`@swarmdrop/shared-view`）——移动端也要用它们。
 * 这里只留**表现层**才有的东西：动作回调与空态插槽，它们的类型里含 `ReactNode`。
 */

import type { ReactNode } from "react";
import type { FileBrowserItem } from "@swarmdrop/shared-view";

/** 一次操作的目标：单个文件，或一整个目录（按相对路径前缀）。 */
export type FileBrowserTarget =
  | { type: "file"; item: FileBrowserItem }
  | { type: "directory"; relativePath: string };

/**
 * 可用动作。**全部由调用方显式传入**——组件不根据 scope 或 mode 猜测业务行为。
 * 不传的动作对应的按钮不渲染，而不是渲染成禁用态。
 *
 * 动作集合是两端的**并集**，不是交集：`onOpen` / `onReveal` 只有桌面给得出（浏览器没有
 * 「在文件夹中显示」），`onDownload` 只有 Web 需要（桌面接收即落盘，没有第二次「取回」
 * 这个动作）。按端裁剪靠「不传就不渲染」，不靠组件里的 `if (platform)`。
 */
export interface FileBrowserActions {
  onRemove?: (target: FileBrowserTarget) => void;
  onOpen?: (item: FileBrowserItem) => void;
  onReveal?: (item: FileBrowserItem) => void;
  onRetry?: (item: FileBrowserItem) => void;
  /** 取回到本机。Web 端把 OPFS 里的文件另存为下载。 */
  onDownload?: (item: FileBrowserItem) => void;
  /**
   * 有取回动作正在执行的条目 ID（`FileBrowserItem.id`）。这些行的下载按钮转成 spinner 并禁用。
   *
   * 传集合而不是 `isPending(item)` 回调：后者每次渲染都是新引用，会把行组件的 `memo`
   * 打穿——而这些行正是在传输中每秒重画的那批。
   */
  pendingIds?: ReadonlySet<string>;
}

export interface FileBrowserEmptyState {
  title: ReactNode;
  description?: ReactNode;
}

/**
 * 缩略图取图源：把 `item.previewSource` 解析成**字节**（`Blob` / `File`）。
 *
 * 返回 `Blob` 而不是 URL，因为缩放管线（`use-thumbnail.ts`）的第一步是
 * `createImageBitmap`，它只吃 `Blob`；给它一个 URL 还得 `fetch` 一次绕回来，
 * 多一次拷贝，中间那个 object URL 也必须记得 revoke。
 *
 * 返回 `null` 表示这一项没有可用预览，组件回落到类型图标。
 *
 * **收的是 `previewSource` 字符串，不是整个 item**：真正需要的只有这一个值，而 item
 * 每次渲染都是新对象（传输中每秒都在重建），传它会逼 hook 再养一个 ref 去躲开依赖。
 *
 * **由调用方注入，组件永远不自己拼路径**（那条边界见
 * `dev-notes/knowledge/file-browser.md` 的「操作与安全边界」）。
 *
 * **桌面不传这个**：它的预览走 `FileBrowserItem.previewUrl`（asset URL 本身就能渲染，
 * 资源协议自己流式读）。需要管线的是 Web——OPFS 里的文件只能异步取成 `File`。
 * 两条路由**字段本身**区分，不由「有没有传本函数」反推。
 *
 * **必须是稳定引用**（`useCallback` 或模块级函数）：它进 hook 的 effect 依赖。
 */
export type ThumbnailResolver = (previewSource: string) => Promise<Blob | null>;
