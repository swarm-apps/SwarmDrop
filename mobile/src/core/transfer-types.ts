/**
 * 移动端传输领域类型 —— RN 侧只消费 projection，不再维护旧 history/status 模型。
 */

import {
  MobileSuspendedReason,
  MobileTerminalReason,
  MobileTransferDirection,
  type MobileTransferOfferFile,
  MobileTransferPhase,
  type MobileTransferProgress,
  type MobileTransferProjection,
} from "react-native-swarmdrop-core";

export type TransferDirection = "send" | "receive";

export type ProjectionStatus =
  | "offered"
  | "waiting_accept"
  | "transferring"
  | "paused"
  | "interrupted"
  | "peer_offline"
  | "app_restarted"
  | "completed"
  | "cancelled"
  | "rejected"
  | "failed";

export type ProjectionGroup =
  | "active"
  | "recoverable"
  | "attention"
  | "completed";

export interface TransferOfferQueueItem {
  id: string;
  offer: {
    sessionId: string;
    peerId: string;
    deviceName: string;
    totalSize: bigint;
    files: MobileTransferOfferFile[];
  };
  receivedAt: number;
}

export function projectionDirection(
  projection: MobileTransferProjection,
): TransferDirection {
  return projection.direction === MobileTransferDirection.Send
    ? "send"
    : "receive";
}

export function projectionStatus(
  projection: MobileTransferProjection,
): ProjectionStatus {
  switch (projection.phase) {
    case MobileTransferPhase.Offered:
      return "offered";
    case MobileTransferPhase.WaitingAccept:
      return "waiting_accept";
    case MobileTransferPhase.Active:
      return "transferring";
    case MobileTransferPhase.Suspended:
      return suspendedStatus(projection.suspendedReason);
    case MobileTransferPhase.Terminal:
      return terminalStatus(projection.terminalReason);
    default:
      return "failed";
  }
}

function suspendedStatus(
  reason: MobileSuspendedReason | undefined,
): ProjectionStatus {
  switch (reason) {
    case MobileSuspendedReason.LocalPaused:
    case MobileSuspendedReason.RemotePaused:
      return "paused";
    case MobileSuspendedReason.PeerOffline:
      return "peer_offline";
    case MobileSuspendedReason.AppRestarted:
      return "app_restarted";
    case MobileSuspendedReason.Interrupted:
      return "interrupted";
    default:
      return "interrupted";
  }
}

function terminalStatus(
  reason: MobileTerminalReason | undefined,
): ProjectionStatus {
  switch (reason) {
    case MobileTerminalReason.Completed:
      return "completed";
    case MobileTerminalReason.Cancelled:
      return "cancelled";
    case MobileTerminalReason.Rejected:
      return "rejected";
    case MobileTerminalReason.FatalError:
      return "failed";
    default:
      return "failed";
  }
}

export function isProjectionActive(
  projection: MobileTransferProjection,
): boolean {
  return (
    projection.phase === MobileTransferPhase.Offered ||
    projection.phase === MobileTransferPhase.WaitingAccept ||
    projection.phase === MobileTransferPhase.Active
  );
}

export function isProjectionRecoverable(
  projection: MobileTransferProjection,
): boolean {
  return (
    projection.phase === MobileTransferPhase.Suspended && projection.recoverable
  );
}

export function isProjectionTerminal(
  projection: MobileTransferProjection,
): boolean {
  return projection.phase === MobileTransferPhase.Terminal;
}

export function projectionNeedsAttention(
  projection: MobileTransferProjection,
): boolean {
  if (projection.phase === MobileTransferPhase.Suspended) {
    return !projection.recoverable;
  }
  return (
    projection.phase === MobileTransferPhase.Terminal &&
    projection.terminalReason === MobileTerminalReason.FatalError
  );
}

export function projectionGroup(
  projection: MobileTransferProjection,
): ProjectionGroup {
  if (isProjectionActive(projection)) return "active";
  if (isProjectionRecoverable(projection)) return "recoverable";
  if (projectionNeedsAttention(projection)) return "attention";
  return "completed";
}

export function groupTransferProjections(
  projections: MobileTransferProjection[],
): Record<ProjectionGroup, MobileTransferProjection[]> {
  const grouped: Record<ProjectionGroup, MobileTransferProjection[]> = {
    active: [],
    recoverable: [],
    attention: [],
    completed: [],
  };

  for (const projection of projections) {
    grouped[projectionGroup(projection)].push(projection);
  }

  for (const items of Object.values(grouped)) {
    items.sort((a, b) => Number(b.updatedAt - a.updatedAt));
  }

  return grouped;
}

export function projectionPolicyNote(
  projection: MobileTransferProjection,
): string | null {
  if (projection.policyReason) {
    return projection.policyAction
      ? `${projection.policyAction}: ${projection.policyReason}`
      : projection.policyReason;
  }
  return projection.policyAction ?? null;
}

export function projectionTransferredBytes(
  projection: MobileTransferProjection,
  progress?: MobileTransferProgress,
): bigint {
  return progress?.transferredBytes ?? projection.transferredBytes;
}

export function projectionTotalBytes(
  projection: MobileTransferProjection,
  progress?: MobileTransferProgress,
): bigint {
  return progress?.totalBytes ?? projection.totalSize;
}
