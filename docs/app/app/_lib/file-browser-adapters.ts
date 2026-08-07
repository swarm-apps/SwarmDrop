/**
 * Web 侧的 file-browser 输入适配。
 *
 * `@swarmdrop/shared-view` 的四个 adapter 只声明**用到的字段**——它不能 import 任何一端的
 * bindings（那会把 wasm-bindgen 的产物变成桌面与移动端的依赖）。所以各端有一份这样的薄适配
 * （桌面是 `src/lib/file-browser-adapters.ts`）。
 *
 * Web 这份很薄：wasm-bindgen 产出的类型与 L1 的入参**结构等价**，枚举也是同一套 wire 字符串，
 * 所以传输那两条就是直传。真正需要转换的只有发送侧（浏览器 `File`）与收件箱的取图源。
 *
 * 本文件只做字段搬运，**不含任何展示判断**——那些在 L1。
 */

import {
  fromInboxFiles as sharedFromInboxFiles,
  fromOfferFiles as sharedFromOfferFiles,
  fromProjectionFiles as sharedFromProjectionFiles,
  fromSelectedFiles as sharedFromSelectedFiles,
  isPathInsideDirectory,
  type FileBrowserItem,
  type OfferFileInput,
} from "@swarmdrop/shared-view";
import type { FileBrowserTarget } from "@swarmdrop/file-browser";
import type {
  InboxItemFileEntry,
  TransferProgressEvent,
  TransferProjection,
} from "./view-types";

/** 发送侧待发文件（浏览器 `File`）。 */
export interface PendingFileInput {
  /** 本地自增序号。同一个文件被选两次时它是唯一能区分两行的东西，故用作来源键。 */
  id: number;
  file: File;
}

/**
 * 发送侧已选文件。
 *
 * `sourceId` 用**自增序号**而不是路径：浏览器允许把同一个文件选两次，用路径当键会让两行
 * 撞同一个展示 ID（React key 与树节点表都会错乱）。目录结构由 `webkitRelativePath` 带出来
 * ——选文件夹时它是「顶层目录名/…/文件名」，选单文件时是空串，故回落 `name`。
 */
export function itemsFromPendingFiles(files: readonly PendingFileInput[]): FileBrowserItem[] {
  return sharedFromSelectedFiles(
    files.map(({ id, file }) => ({
      sourceId: String(id),
      name: file.name,
      relativePath: file.webkitRelativePath || file.name,
      size: file.size,
      // 取图源也是自增序号：待发文件的字节只活在内存里的那个 `File` 句柄上，
      // **没有任何路径指得到它**，所以这里存的是回查用的键，由
      // `createPendingFileThumbnailSource` 换回 `File`。
      previewSource: String(id),
    })),
  );
}

/**
 * 入站 offer 的文件清单。
 *
 * 入参按 L1 的同一条约定**只声明用到的字段**：Web 侧的 offer 有两个来源——事件流的
 * `TransferOfferEvent`（带 `isDirectory`）与 `pending_offers()` 快照的 `OfferJson`
 * （`FileInfo` 没有这个字段，wire 上根本不传目录条目）。store 在边界把两者归一成
 * `IncomingOffer`，取的是交集，所以这里的 `isDirectory` 必须是可选的。
 */
export function itemsFromOffer(
  sessionId: string,
  files: readonly OfferFileInput[],
): FileBrowserItem[] {
  return sharedFromOfferFiles(sessionId, files);
}

/**
 * 传输投影 + 实时进度。
 *
 * 这里是 Web 端那个「串会话」故障的根治点：此前渲染层写的是
 * `live?.files ?? projection.files`——进度与投影**二选一**，于是同一份数据有两种形状
 * （`transferred` vs `transferredBytes`），消费点得靠 `"transferred" in file` 现场嗅探，
 * 而进度域是按 sessionId 常驻的，切换会话时那一路取到的可能是另一条会话的采样。
 *
 * 现在投影是骨架、进度只是覆盖层，行数与身份**只由 projection 决定**；终态忽略进度
 * 的判定也收在 L1 里（不再依赖调用方自觉带上 `transferSample` 的 `live`）。
 */
export function itemsFromProjection(
  projection: TransferProjection,
  progress?: TransferProgressEvent | null,
): FileBrowserItem[] {
  // 三个入参都是**结构等价直传**：`TransferProjectionFile` ≡ `ProjectionFileInput`、
  // `FileProgressInfo` 是 `ProgressFileInput` 的超集、`TerminalReason` 与
  // `SessionTerminalReason` 是同一套字面量（L1 沿用了 wire 的拼写）。中间加一层恒等
  // `.map()` 只是每次渲染多复制一遍整个数组，而这条路径每秒都在跑。
  return sharedFromProjectionFiles(projection.sessionId, projection.files, {
    phase: projection.phase,
    terminalReason: projection.terminalReason,
    progress: progress?.files,
  });
}

/**
 * 收件箱条目的文件行。
 *
 * `sourceId` 是收件箱文件行主键（下载按它定位），`relativePath` 是 OPFS 的键。
 * **不要用 `localPath`**——Web 上它是带 `opfs:/` 前缀的展示值（见 DTO 注释）。
 */
export function itemsFromInbox(
  itemId: string,
  files: readonly InboxItemFileEntry[],
): FileBrowserItem[] {
  return sharedFromInboxFiles(
    itemId,
    // 除取图源外字段一一对应，直接透传。取图源是 OPFS 的键，与
    // `download_url(file.relativePath)` 用的是同一个字段——**不要用 `localPath`**，
    // Web 上它是带 `opfs:/` 前缀的展示值。缺失的文件由 L1 自动丢掉这一项。
    files.map((file) => ({ ...file, previewSource: file.relativePath })),
  );
}

/**
 * 从待发清单里移除一个目标（单个文件或整个目录）—— `itemsFromPendingFiles` 的逆运算。
 *
 * **文件按来源键移除，不按路径**：浏览器允许把同一个文件选两次，两行的 `relativePath`
 * 一模一样，按路径删会把它们一起带走。目录则按路径前缀，判据走共享的
 * `isPathInsideDirectory`（它按 segment 边界比，不会让 `a/b` 误匹配 `a/bc`）。
 */
export function removePendingTarget(
  files: readonly PendingFileInput[],
  target: FileBrowserTarget,
): PendingFileInput[] {
  if (target.type === "directory") {
    return files.filter(
      ({ file }) =>
        !isPathInsideDirectory(
          file.webkitRelativePath || file.name,
          target.relativePath,
        ),
    );
  }
  return files.filter(({ id }) => String(id) !== target.item.sourceId);
}
