/**
 * 移动端传输领域类型 —— 桥接 mobile-core uniffi 类型与 RN store / UI 形态。
 *
 * 桌面端有 `@/lib/types#TransferSession` 做同样的事。本文件移动端对齐：
 * - 活跃 session 的运行时形态（含 progress 实时快照）
 * - active / history 共享的状态字面量与方向字面量
 * - 与 mobile-core 共享的错误码常量
 */

import type {
  MobileTransferOfferFile,
  MobileTransferProgress,
} from "react-native-swarmdrop-core";

/**
 * 与 mobile-core 共享的错误码字面量。
 *
 * 源头：`packages/swarmdrop-core/rust/mobile-core/src/error.rs` 的
 * `ERROR_APP_INTERRUPTED`。reconcile 把残留 transferring 状态写进 DB 时用，
 * UI 端通过这个字面量做 i18n 映射（zh "上次未完成" / en "Interrupted"）。
 */
export const ERROR_APP_INTERRUPTED = "app_interrupted";

/** 与 entity::SessionStatus 对齐的 5 个 DB 状态 */
export type SessionStatus =
  | "transferring"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

/** 活跃 session UI 形态可能出现的额外中间态（不进 DB） */
export type ActiveStatus = "waiting_accept" | SessionStatus;

export type TransferDirection = "send" | "receive";

/** 活跃 session 持有的单个文件元数据（来自 offer / prepared） */
export interface SessionFile {
  fileId: number;
  name: string;
  relativePath: string;
  size: bigint;
  isDirectory: boolean;
}

/**
 * 活跃传输会话（仅内存）。
 *
 * 进入条件：
 * - 发送方：`sendPrepared` 成功后由 store.addSession 注册
 * - 接收方：`acceptReceive` 成功后由 store.addSession 注册（基于入站 offer 拼装）
 *
 * 退出条件：
 * - TransferCompleted / TransferFailed / TransferPaused 事件 → `removeAndRefresh`
 *   先 await loadHistory() 再删除（保证 UI 不空窗）
 */
export interface TransferSession {
  sessionId: string;
  direction: TransferDirection;
  peerId: string;
  peerName: string;
  files: SessionFile[];
  totalSize: bigint;
  status: ActiveStatus;
  /** 最新一次 TransferProgress 事件快照，含 speed/eta */
  progress: MobileTransferProgress | null;
  error: string | null;
  startedAt: number;
  completedAt: number | null;
}

/**
 * 入站 offer 队列条目 —— 接收方在 dialog 显示之前的待响应快照。
 */
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

/** Active session 的最初 metadata（addSession 入参） */
export interface RegisterSessionInput {
  sessionId: string;
  direction: TransferDirection;
  peerId: string;
  peerName: string;
  files: SessionFile[];
  totalSize: bigint;
  /** 默认 send → "waiting_accept"、receive → "transferring"；resume 路径可显式传 "transferring" */
  initialStatus?: ActiveStatus;
}
