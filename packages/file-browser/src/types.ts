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
  /**
   * 取回到本机（Web 端唯一的取出口；桌面接收即落盘，不传）。
   *
   * **与 `onRemove` 同为 target 形态，不是 `(item)`**，且这是一条契约而不只是签名：
   * 目录是一个独立目标，调用方**不得**把它拆成「循环调 N 次文件取回」。为什么那样做不
   * 可行是各端自己的事（Web 端的推导与实现见 `docs/app/app/_lib/zip-download.ts`），
   * 本包只要求这个动作能被单独实现。
   */
  onDownload?: (target: FileBrowserTarget) => void;
  /**
   * 转发到另一台设备（收件箱专用）。
   *
   * 与 `onDownload` 是两个方向：那个把文件取回本机，这个把它送去别处。浏览器里两者都得
   * 由应用提供——OPFS 对用户不可见，系统 picker 选不到它。
   */
  onSend?: (item: FileBrowserItem) => void;
  /**
   * 有取回动作正在执行的目标。这些行的下载按钮转成 spinner 并禁用。
   *
   * **放的是 target 自报的身份**：文件是 `item.id`，目录是 `relativePath`（带尾斜杠）
   * ——也就是调用方在 `onDownload(target)` 里**原样收到**的那个值。两种身份共用一个集合，
   * 因为「谁在转圈」这件事对调用方只有一份；而用 target 里已有的字段，调用方就不必知道
   * 建树时怎么给节点编 id。
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
