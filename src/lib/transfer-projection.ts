import { t } from "@lingui/core/macro";
import type { TransferProjection } from "@/lib/bindings";

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

/** 终态且成功完成。 */
export function isProjectionCompleted(
  projection: TransferProjection,
): boolean {
  return (
    projection.phase === "terminal" &&
    projection.terminalReason === "completed"
  );
}

/** 终态且被取消（本端取消或对方拒绝）。 */
export function isProjectionCancelled(
  projection: TransferProjection,
): boolean {
  return (
    projection.phase === "terminal" &&
    (projection.terminalReason === "cancelled" ||
      projection.terminalReason === "rejected")
  );
}

/** 终态且失败（既非完成也非取消，对应 fatal_error 等）。 */
export function isProjectionFailed(projection: TransferProjection): boolean {
  return (
    projection.phase === "terminal" &&
    !isProjectionCompleted(projection) &&
    !isProjectionCancelled(projection)
  );
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
