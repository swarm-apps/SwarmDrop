import { t } from "@lingui/core/macro";
import type {
  TransferProgressEvent,
  TransferProjection,
  TransferProjectionFile,
} from "@/lib/bindings";
import type { TransferSession, TransferStatus } from "@/lib/types";

export type ProjectionStatusFilter =
  | "all"
  | "completed"
  | "suspended"
  | "cancelled"
  | "failed";

export function isProjectionActive(projection: TransferProjection): boolean {
  return (
    projection.phase === "offered" ||
    projection.phase === "waiting_accept" ||
    projection.phase === "active"
  );
}

export function projectionToStatus(
  projection: TransferProjection,
): TransferStatus {
  switch (projection.phase) {
    case "offered":
      return "pending";
    case "waiting_accept":
      return "waiting_accept";
    case "active":
      return "transferring";
    case "suspended":
      return "paused";
    case "terminal":
      switch (projection.terminalReason) {
        case "completed":
          return "completed";
        case "cancelled":
        case "rejected":
          return "cancelled";
        case "fatal_error":
        default:
          return "failed";
      }
  }
}

export function projectionStatusLabel(
  projection: TransferProjection,
): string {
  switch (projection.phase) {
    case "offered":
      return t`等待中`;
    case "waiting_accept":
      return t`等待确认`;
    case "active":
      return t`传输中`;
    case "suspended":
      switch (projection.suspendedReason) {
        case "local_paused":
          return t`已暂停`;
        case "remote_paused":
          return t`对方暂停`;
        case "interrupted":
          return t`已中断`;
        case "peer_offline":
          return t`对方离线`;
        case "app_restarted":
          return t`应用重启后中断`;
        default:
          return projection.recoverable ? t`可恢复失败` : t`不可恢复失败`;
      }
    case "terminal":
      switch (projection.terminalReason) {
        case "completed":
          return t`已完成`;
        case "cancelled":
          return t`已取消`;
        case "rejected":
          return t`对方拒绝`;
        case "fatal_error":
        default:
          return t`不可恢复失败`;
      }
  }
}

export function canResumeProjection(projection: TransferProjection): boolean {
  return projection.phase === "suspended" && projection.recoverable;
}

export function projectionMatchesFilter(
  projection: TransferProjection,
  filter: ProjectionStatusFilter,
): boolean {
  if (filter === "all") return true;
  if (filter === "completed") {
    return (
      projection.phase === "terminal" &&
      projection.terminalReason === "completed"
    );
  }
  if (filter === "suspended") return projection.phase === "suspended";
  if (filter === "cancelled") {
    return (
      projection.phase === "terminal" &&
      (projection.terminalReason === "cancelled" ||
        projection.terminalReason === "rejected")
    );
  }
  return (
    projection.phase === "terminal" &&
    projection.terminalReason === "fatal_error"
  );
}

export function projectionToSession(
  projection: TransferProjection,
  progress: TransferProgressEvent | null = null,
): TransferSession {
  return {
    sessionId: projection.sessionId,
    direction: projection.direction,
    peerId: projection.peerId,
    deviceName: projection.peerName,
    files: projection.files.map(projectionFileToTransferFile),
    totalSize: projection.totalSize,
    status: projectionToStatus(projection),
    phase: projection.phase,
    suspendedReason: projection.suspendedReason,
    terminalReason: projection.terminalReason,
    recoverable: projection.recoverable,
    epoch: projection.epoch,
    progress,
    transferredBytes: projection.transferredBytes,
    error: projection.errorMessage,
    startedAt: projection.startedAt,
    updatedAt: projection.updatedAt,
    completedAt: projection.finishedAt,
    saveLocation: projection.savePath ?? undefined,
  };
}

export function applyProgressToProjection(
  projection: TransferProjection,
  progress: TransferProgressEvent,
): TransferProjection {
  const transferredByFile = new Map(
    progress.files.map((file) => [file.fileId, file.transferred]),
  );

  return {
    ...projection,
    transferredBytes: progress.transferredBytes,
    files: projection.files.map((file) => ({
      ...file,
      transferredBytes:
        transferredByFile.get(file.fileId) ?? file.transferredBytes,
    })),
  };
}

function projectionFileToTransferFile(file: TransferProjectionFile) {
  return {
    fileId: file.fileId,
    name: file.name,
    relativePath: file.relativePath,
    size: file.size,
    isDirectory: false,
  };
}
