/**
 * 展示 ID、路径归一与**已选文件集合**的操作。
 *
 * 前一半（`normalizeRelativePath` / 三个 ID 构造器 / `isPathInsideDirectory`）已上移到
 * `@swarmdrop/shared-view`——三端要产出同一个展示 ID，各写一份迟早分叉。这里只作转发，
 * 保留本模块路径是为了不动几十个调用点。
 *
 * 上移顺带修严了一处：共享版的归一还会滤掉 `.` 与 `..` 段。这个字符串会被各端当作落盘的
 * 相对路径用，`..` 是路径穿越的载体。
 *
 * 后一半留在这里：它们操作的是 `MobileTransferFile`（uniffi 产物），进不了那个平台中立的包。
 */

export {
  inboxFileId,
  isPathInsideDirectory,
  normalizeDirectoryPath,
  normalizeRelativePath,
  selectedFileId,
  sessionFileId,
} from "@swarmdrop/shared-view";

import { isPathInsideDirectory } from "@swarmdrop/shared-view";
import type { MobileTransferFile } from "react-native-swarmdrop-core";

export function mergeSelectedFiles(
  current: readonly MobileTransferFile[],
  incoming: readonly MobileTransferFile[],
): MobileTransferFile[] {
  const seen = new Set(current.map((file) => file.sourceId));
  const merged = [...current];
  for (const file of incoming) {
    if (seen.has(file.sourceId)) continue;
    merged.push(file);
    seen.add(file.sourceId);
  }
  return merged;
}

export function removeSelectedFile(
  files: readonly MobileTransferFile[],
  sourceId: string,
): MobileTransferFile[] {
  return files.filter((file) => file.sourceId !== sourceId);
}

export function removeSelectedDirectory(
  files: readonly MobileTransferFile[],
  relativeDirectory: string,
): MobileTransferFile[] {
  return files.filter(
    (file) => !isPathInsideDirectory(file.relativePath, relativeDirectory),
  );
}
