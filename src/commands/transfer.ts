/**
 * Transfer commands
 * 文件传输相关类型定义和命令
 */

import { Channel, invoke } from "@tauri-apps/api/core";

// === 类型定义 ===

/** 保存位置 */
export type SaveLocation = { type: "path"; path: string };

/** 传输方向 */
export type TransferDirection = "send" | "receive";

/** 传输状态 */
export type TransferStatus =
  | "pending"
  | "waiting_accept"
  | "transferring"
  | "completed"
  | "failed"
  | "cancelled";

/** 文件信息 */
export interface TransferFileInfo {
  fileId: number;
  name: string;
  relativePath: string;
  size: number;
  isDirectory: boolean;
}

/** 准备发送的结果 */
export interface PreparedTransfer {
  preparedId: string;
  files: TransferFileInfo[];
  totalSize: number;
}

/** 传输会话 */
export interface TransferSession {
  sessionId: string;
  direction: TransferDirection;
  peerId: string;
  deviceName: string;
  files: TransferFileInfo[];
  totalSize: number;
  status: TransferStatus;
  progress: TransferProgressEvent | null;
  error: string | null;
  startedAt: number;
  completedAt: number | null;
  saveLocation?: SaveLocation;
}

// === 事件类型 ===

/** 接收方收到传输提议 */
export interface TransferOfferEvent {
  sessionId: string;
  peerId: string;
  deviceName: string;
  files: TransferFileInfo[];
  totalSize: number;
}

/** 单个文件的进度信息 */
export interface FileProgressInfo {
  fileId: number;
  name: string;
  size: number;
  transferred: number;
  status: "pending" | "transferring" | "completed";
}

/** 传输进度更新 */
export interface TransferProgressEvent {
  sessionId: string;
  direction: TransferDirection;
  totalFiles: number;
  completedFiles: number;
  totalBytes: number;
  transferredBytes: number;
  speed: number;
  eta: number | null;
  /** 每个文件的独立进度 */
  files: FileProgressInfo[];
}

/** 传输完成 */
export interface TransferCompleteEvent {
  sessionId: string;
  direction: TransferDirection;
  totalBytes: number;
  elapsedMs: number;
  saveLocation?: SaveLocation;
}

/** 传输失败 */
export interface TransferFailedEvent {
  sessionId: string;
  direction: TransferDirection;
  error: string;
}

/** 对端暂停传输 */
export interface TransferPausedEvent {
  sessionId: string;
  direction: TransferDirection;
}

/** 对端发起断点续传，本地自动恢复 */
export interface TransferResumedEvent {
  sessionId: string;
  direction: TransferDirection;
  peerId: string;
  peerName: string;
  files: TransferFileInfo[];
  totalSize: number;
}

// === 文件来源 ===

/**
 * 文件来源（与 Rust FileSource 枚举对应）
 */
export type FileSource = { type: "path"; path: string };

// === 扫描结果 ===

/** 单个来源的扫描结果 */
export interface ScannedSourceResult {
  isDirectory: boolean;
  files: ScannedFile[];
  totalSize: number;
}

/** 扫描到的单个文件（同时用于 scanSources 返回和 prepareSend 输入） */
export interface ScannedFile {
  source: FileSource;
  name: string;
  relativePath: string;
  size: number;
}

/** prepare_send 进度事件 */
export interface PrepareProgress {
  /** 当前正在 hash 的文件名 */
  currentFile: string;
  /** 已完成 hash 的文件数 */
  completedFiles: number;
  /** 总文件数 */
  totalFiles: number;
  /** 累积已 hash 的字节数（所有文件） */
  bytesHashed: number;
  /** 总字节数（所有文件） */
  totalBytes: number;
}

// === 命令函数 ===

/** Offer 被拒绝的原因（与 Rust OfferRejectReason 对应） */
export type OfferRejectReason =
  | { type: "not_paired" }
  | { type: "user_declined" };

/** 开始发送的结果（立即返回 session_id，后续通过事件通知） */
export interface StartSendResult {
  sessionId: string;
}

/** 对方接受 Offer 的事件 */
export interface TransferAcceptedEvent {
  sessionId: string;
}

/** 对方拒绝 Offer 的事件 */
export interface TransferRejectedEvent {
  sessionId: string;
  reason: OfferRejectReason | null;
}

/** DB 操作失败事件（传输记录保存失败时触发） */
export interface TransferDbErrorEvent {
  sessionId: string;
  message: string;
}

/** 恢复传输的结果（返回给前端以创建运行时 session） */
export interface ResumeTransferResult {
  sessionId: string;
  direction: string;
  peerId: string;
  peerName: string;
  files: TransferFileInfo[];
  totalSize: number;
  transferredBytes: number;
}

/**
 * 扫描文件来源：遍历目录、收集元数据，不计算 hash
 * 用于用户选择文件后在 UI 上展示文件树
 */
export async function scanSources(
  sources: FileSource[],
): Promise<ScannedSourceResult[]> {
  return invoke("scan_sources", { sources });
}

/**
 * 准备发送：对预扫描的文件列表计算 BLAKE3 校验和
 * 接收 scanSources 返回的 ScannedFile 列表（前端可能已移除部分文件）
 * @param onProgress 可选的进度回调，实时接收 hash 计算进度
 */
export async function prepareSend(
  files: ScannedFile[],
  onProgress?: (progress: PrepareProgress) => void,
): Promise<PreparedTransfer> {
  const channel = new Channel<PrepareProgress>();
  if (onProgress) {
    channel.onmessage = onProgress;
  }
  return invoke("prepare_send", { files, onProgress: channel });
}

/** 开始发送到指定设备，等待对方响应 */
export async function startSend(
  preparedId: string,
  peerId: string,
  peerName: string,
  selectedFileIds: number[],
): Promise<StartSendResult> {
  return invoke("start_send", { preparedId, peerId, peerName, selectedFileIds });
}

/** 取消发送 */
export async function cancelSend(sessionId: string): Promise<void> {
  return invoke("cancel_send", { sessionId });
}

/** 确认接收 */
export async function acceptReceive(
  sessionId: string,
  saveLocation: SaveLocation,
): Promise<void> {
  return invoke("accept_receive", { sessionId, saveLocation });
}

/** 拒绝接收 */
export async function rejectReceive(sessionId: string): Promise<void> {
  return invoke("reject_receive", { sessionId });
}

/** 取消接收 */
export async function cancelReceive(sessionId: string): Promise<void> {
  return invoke("cancel_receive", { sessionId });
}

// === 传输历史 API ===

/** 历史会话状态（对应 Rust SessionStatus） */
export type HistorySessionStatus =
  | "transferring"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

/** 历史文件状态（对应 Rust FileStatus） */
export type HistoryFileStatus =
  | "pending"
  | "transferring"
  | "completed"
  | "failed";

/** 传输历史文件记录 */
export interface TransferHistoryFile {
  fileId: number;
  name: string;
  relativePath: string;
  size: number;
  status: HistoryFileStatus;
  transferredBytes: number;
}

/** 传输历史会话记录（对应 Rust TransferHistoryItem） */
export interface TransferHistoryItem {
  sessionId: string;
  direction: TransferDirection;
  peerId: string;
  peerName: string;
  totalSize: number;
  transferredBytes: number;
  status: HistorySessionStatus;
  startedAt: number;
  updatedAt: number;
  finishedAt: number | null;
  errorMessage: string | null;
  savePath: SaveLocation | null;
  files: TransferHistoryFile[];
}

/** 查询传输历史列表（可选按状态过滤） */
export async function getTransferHistory(
  status?: HistorySessionStatus,
): Promise<TransferHistoryItem[]> {
  return invoke("get_transfer_history", { status: status ?? null });
}

/** 查询单个传输会话详情 */
export async function getTransferSession(
  sessionId: string,
): Promise<TransferHistoryItem> {
  return invoke("get_transfer_session", { sessionId });
}

/** 删除单个传输会话 */
export async function deleteTransferSession(
  sessionId: string,
): Promise<void> {
  return invoke("delete_transfer_session", { sessionId });
}

/** 清空所有传输历史 */
export async function clearTransferHistory(): Promise<void> {
  return invoke("clear_transfer_history");
}

/** 暂停传输 */
export async function pauseTransfer(sessionId: string): Promise<void> {
  return invoke("pause_transfer", { sessionId });
}

/** 恢复传输（断点续传） */
export async function resumeTransfer(
  sessionId: string,
): Promise<ResumeTransferResult> {
  return invoke("resume_transfer", { sessionId });
}
