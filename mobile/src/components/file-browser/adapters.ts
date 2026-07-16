import {
  type MobileInboxFileEntry,
  MobileTerminalReason,
  type MobileTransferFile,
  type MobileTransferOfferFile,
  MobileTransferPhase,
  type MobileTransferProgress,
  type MobileTransferProjection,
  type MobileTransferProjectionFile,
} from "react-native-swarmdrop-core";
import {
  inboxFileId,
  normalizeRelativePath,
  selectedFileId,
  sessionFileId,
} from "@/core/file-browser-identity";
import type { FileBrowserItem, FileBrowserStatus } from "./types";

export function fromSelectedFiles(
  files: readonly MobileTransferFile[],
): FileBrowserItem[] {
  return files.map((file) => ({
    id: selectedFileId(file.sourceId),
    sourceId: file.sourceId,
    name: file.name,
    relativePath: normalizeRelativePath(file.relativePath, file.name),
    size: file.size,
    status: "idle",
    // 发送来源在所有选择路径下都是可渲染的 file://(见 core/file-access + share-intent)。
    localUri: file.sourceId,
  }));
}

export function fromOfferFiles(
  sessionId: string,
  files: readonly MobileTransferOfferFile[],
): FileBrowserItem[] {
  return files.flatMap((file) => {
    if (file.isDirectory) return [];
    return [
      {
        id: sessionFileId(sessionId, file.fileId),
        fileId: file.fileId,
        name: file.name,
        relativePath: normalizeRelativePath(
          file.relativePath ?? file.name,
          file.name,
        ),
        size: file.size,
        status: "waiting" as const,
      },
    ];
  });
}

export function fromProjection(
  projection: MobileTransferProjection,
  progress?: MobileTransferProgress | null,
): FileBrowserItem[] {
  const progressByFileId = new Map(
    progress?.files.map((file) => [file.fileId, file]) ?? [],
  );
  return projection.files.map((file) => {
    const live = progressByFileId.get(file.fileId);
    const transferred = live?.transferred ?? file.transferredBytes;
    const status = projectionFileStatus(
      projection,
      file,
      transferred,
      live?.status,
    );
    return {
      id: sessionFileId(projection.sessionId, file.fileId),
      fileId: file.fileId,
      name: file.name,
      relativePath: normalizeRelativePath(file.relativePath, file.name),
      size: file.size,
      status,
      ...(status === "transferring" || status === "paused"
        ? { progress: progressPercent(transferred, file.size) }
        : {}),
    };
  });
}

export function fromInboxFiles(
  itemId: string,
  files: readonly MobileInboxFileEntry[],
): FileBrowserItem[] {
  return files.map((file) => ({
    id: inboxFileId(itemId, file.id),
    fileId: file.transferFileId ?? file.id,
    name: file.name,
    relativePath: normalizeRelativePath(file.relativePath, file.name),
    size: file.size,
    status: file.missing ? "missing" : "completed",
    // 仅未缺失且真为 file:// 时可缩略图;Android SAF content:// 不设(交系统打开)。
    ...(!file.missing && file.localPath.startsWith("file://")
      ? { localUri: file.localPath }
      : {}),
  }));
}

function projectionFileStatus(
  projection: MobileTransferProjection,
  file: MobileTransferProjectionFile,
  transferred: bigint,
  liveStatus?: string,
): FileBrowserStatus {
  if (liveStatus === "completed") return "completed";
  if (liveStatus === "transferring") return "transferring";
  if (liveStatus === "failed" || liveStatus === "error") return "error";

  const complete = file.size > 0n && transferred >= file.size;
  if (complete) return "completed";

  switch (projection.phase) {
    case MobileTransferPhase.Active:
      return transferred > 0n ? "transferring" : "waiting";
    case MobileTransferPhase.Suspended:
      return transferred > 0n ? "paused" : "waiting";
    case MobileTransferPhase.Terminal:
      if (projection.terminalReason === MobileTerminalReason.Completed) {
        return "completed";
      }
      if (
        projection.terminalReason === MobileTerminalReason.Cancelled ||
        projection.terminalReason === MobileTerminalReason.Rejected
      ) {
        return "cancelled";
      }
      return "error";
    case MobileTransferPhase.Offered:
    case MobileTransferPhase.WaitingAccept:
      return "waiting";
    default:
      return "waiting";
  }
}

function progressPercent(transferred: bigint, size: bigint): number {
  if (size <= 0n) return 0;
  const basisPoints = (transferred * 10_000n) / size;
  return Math.min(100, Number(basisPoints) / 100);
}
