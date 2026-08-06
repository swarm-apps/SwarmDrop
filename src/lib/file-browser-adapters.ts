/**
 * 桌面侧的 file-browser 输入适配。
 *
 * `@swarmdrop/shared-view` 的四个 adapter 只声明**用到的字段**——它不能 import 任何一端的
 * bindings（那会把 tauri-specta 的产物变成移动端与 Web 的依赖）。所以「哪一端叫什么」的
 * 知识落在这里，三端各有一份这样的薄适配。
 *
 * 桌面这份很薄：specta 产出的类型与 L1 的入参**结构等价**，枚举也是同一套 wire 字符串，
 * 所以多数函数就是直传。真正需要转换的只有收件箱那条取图路径。
 *
 * 本文件只做字段搬运与取图源解析，**不含任何展示判断**——那些在 L1。
 */

import {
  fromInboxFiles as sharedFromInboxFiles,
  fromOfferFiles as sharedFromOfferFiles,
  fromProjectionFiles as sharedFromProjectionFiles,
  fromSelectedFiles as sharedFromSelectedFiles,
  isImageFile,
  type FileBrowserItem,
} from "@swarmdrop/shared-view";
import { convertFileSrc } from "@tauri-apps/api/core";
import type {
  EnumeratedFile,
  InboxItemFileEntry,
  TransferOfferFileEvent,
  TransferProgressEvent,
  TransferProjection,
} from "@/lib/bindings";

/** 发送侧扫描到的文件。`source.path` 同时充当来源键与展示 ID 的基础。 */
export function itemsFromScannedFiles(files: EnumeratedFile[]): FileBrowserItem[] {
  return sharedFromSelectedFiles(
    files.map((file) => ({
      sourceId: file.source.path || file.relativePath || file.name,
      name: file.name,
      relativePath: file.relativePath || file.name,
      size: file.size,
      // 发送任意本地文件**不给取图源**——为预览扩大 Tauri 的 asset scope 是安全边界的
      // 后退（见 dev-notes/knowledge/file-browser.md 的「操作与安全边界」）。
    })),
  );
}

/**
 * 入站 offer 的文件清单。
 *
 * `TransferOfferFileEvent` 与 L1 的 `OfferFileInput` 字段一一对应，直传即可——中间加一层
 * 恒等 `.map()` 只是每次渲染多复制一遍整个数组。
 */
export function itemsFromOffer(
  sessionId: string,
  files: TransferOfferFileEvent[],
): FileBrowserItem[] {
  return sharedFromOfferFiles(sessionId, files);
}

/**
 * 传输投影 + 实时进度。progress 是覆盖层，终态时由 L1 自动忽略。
 *
 * 三个入参同样是结构等价直传：`TransferProjectionFile` ≡ `ProjectionFileInput`，
 * `FileProgressInfo` 是 `ProgressFileInput` 的超集，`TerminalReason` 与
 * `SessionTerminalReason` 是同一套字面量（L1 刻意沿用了 wire 的拼写，`fatal_error` 也不例外）。
 */
export function itemsFromProjection(
  projection: TransferProjection,
  progress?: TransferProgressEvent | null,
): FileBrowserItem[] {
  return sharedFromProjectionFiles(projection.sessionId, projection.files, {
    phase: projection.phase,
    terminalReason: projection.terminalReason,
    progress: progress?.files,
  });
}

/**
 * 收件箱条目的文件行。
 *
 * 取图在这里解析成 asset URL 并存进 **`previewUrl`**（可直接渲染），不是 `previewSource`
 * （那一路要经 resolver + 缩放管线，是 Web 的形态）。收件箱的文件本就在 asset scope 内
 * （是本端接收落盘的），与发送侧「任意本地文件」的处境不同。
 */
export function itemsFromInbox(
  itemId: string,
  files: InboxItemFileEntry[],
): FileBrowserItem[] {
  return sharedFromInboxFiles(
    itemId,
    files.map((file) => ({ ...file, previewUrl: previewUrlOf(file) })),
  );
}

function previewUrlOf(file: InboxItemFileEntry): string | undefined {
  if (file.missing || !isImageFile(file.name)) return undefined;
  try {
    return convertFileSrc(file.localPath);
  } catch {
    return undefined;
  }
}
