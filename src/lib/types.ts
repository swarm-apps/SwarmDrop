/**
 * 前端补充类型
 *
 * bindings.ts 没有、但前端业务需要的类型：业务别名、聚合类型、协议 string 类型。
 */

import type {
  CoreSaveLocation,
  EnumeratedFile,
  PrepareProgressEvent,
  SessionStatus,
  TransferFileResult,
  TransferProgressEvent,
} from "@/lib/bindings";

// ── 业务别名 ──────────────────────────────────────────────────────────

export type SaveLocation = CoreSaveLocation;
export type ScannedFile = EnumeratedFile;
export type PrepareProgress = PrepareProgressEvent;
export type HistorySessionStatus = SessionStatus;

// ── 协议层 string ────────────────────────────────────────────────────

/** Peer ID (libp2p 节点标识) */
export type PeerId = string;

// ── 前端聚合类型 ─────────────────────────────────────────────────────

/** 传输状态（前端运行时聚合，包含 waiting_accept 等过渡态） */
export type TransferStatus =
  | "pending"
  | "waiting_accept"
  | "transferring"
  | "completed"
  | "failed"
  | "cancelled";

/** 传输会话（前端聚合：DB 历史 + 实时进度 + UI 过渡状态） */
export interface TransferSession {
  sessionId: string;
  direction: "send" | "receive";
  peerId: string;
  deviceName: string;
  files: TransferFileResult[];
  totalSize: number;
  status: TransferStatus;
  progress: TransferProgressEvent | null;
  error: string | null;
  startedAt: number;
  completedAt: number | null;
  saveLocation?: SaveLocation;
}
