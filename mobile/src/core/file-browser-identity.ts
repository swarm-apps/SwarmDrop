import type { MobileTransferFile } from "react-native-swarmdrop-core";

export function normalizeRelativePath(path: string, fallbackName = "file") {
  const normalized = path
    .replaceAll("\\", "/")
    .split("/")
    .filter((segment) => segment.length > 0)
    .join("/");
  return normalized || fallbackName || "file";
}

export function normalizeDirectoryPath(path: string): string {
  return path
    .replaceAll("\\", "/")
    .split("/")
    .filter((segment) => segment.length > 0)
    .join("/");
}

export function selectedFileId(sourceId: string): string {
  return `source:${sourceId}`;
}

export function sessionFileId(sessionId: string, fileId: number): string {
  return `session:${sessionId}:file:${fileId}`;
}

export function inboxFileId(itemId: string, fileId: number): string {
  return `inbox:${itemId}:file:${fileId}`;
}

export function isPathInsideDirectory(
  relativePath: string,
  relativeDirectory: string,
): boolean {
  const path = normalizeRelativePath(relativePath);
  const directory = normalizeDirectoryPath(relativeDirectory);
  return (
    directory.length > 0 &&
    (path === directory || path.startsWith(`${directory}/`))
  );
}

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
