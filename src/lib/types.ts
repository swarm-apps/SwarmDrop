/**
 * 前端补充类型
 *
 * bindings.ts 没有、但前端业务需要的类型：业务别名、聚合类型、协议 string 类型。
 */

import type {
  CoreSaveLocation,
  EnumeratedFile,
  PrepareProgressEvent,
} from "@/lib/bindings";

// ── 业务别名 ──────────────────────────────────────────────────────────

export type SaveLocation = CoreSaveLocation;
export type ScannedFile = EnumeratedFile;
export type PrepareProgress = PrepareProgressEvent;

// ── 协议层 string ────────────────────────────────────────────────────

/** Peer ID (libp2p 节点标识) */
export type PeerId = string;
