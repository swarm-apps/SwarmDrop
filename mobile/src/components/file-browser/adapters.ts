/**
 * 移动侧的 file-browser 输入适配。
 *
 * 归一逻辑本身已上移到 `@swarmdrop/shared-view`（本文件原先那份就是三端的蓝本——它已经把
 * 「progress 是投影之上的覆盖层」和完整的 `phase × terminalReason → status` 映射做对了）。
 * 这里只剩两件本端特有的事，三端各有一份这样的薄适配：
 *
 * 1. **枚举映射**：uniffi 给的是 `MobileTransferPhase.Active` 这类 enum 成员，共享包用中立
 *    字面量。把「哪一端叫什么」放进那个包，等于让它认识三套 codegen 的产物。
 * 2. **`bigint → number`**：uniffi 的 `u64` 映射成 `bigint`，共享模型统一用 `number`
 *    （文件大小碰不到 `Number.MAX_SAFE_INTEGER` = 9 PB）。转换收在这一层，
 *    表现层不再有任何 `0n` 与 `BigInt` 混算。
 */

import {
  type FileBrowserItem,
  type ProgressFileStatus,
  type SessionPhase,
  type SessionTerminalReason,
  fromInboxFiles as sharedFromInboxFiles,
  fromOfferFiles as sharedFromOfferFiles,
  fromProjectionFiles as sharedFromProjectionFiles,
  fromSelectedFiles as sharedFromSelectedFiles,
} from "@swarmdrop/shared-view";
import {
  type MobileInboxFileEntry,
  MobileTerminalReason,
  type MobileTransferFile,
  type MobileTransferOfferFile,
  MobileTransferPhase,
  type MobileTransferProgress,
  type MobileTransferProjection,
} from "react-native-swarmdrop-core";

/** uniffi 的阶段枚举 → 共享包的中立字面量。 */
function toSharedPhase(phase: MobileTransferPhase): SessionPhase {
  switch (phase) {
    case MobileTransferPhase.Offered:
      return "offered";
    case MobileTransferPhase.WaitingAccept:
      return "waiting_accept";
    case MobileTransferPhase.Active:
      return "active";
    case MobileTransferPhase.Suspended:
      return "suspended";
    case MobileTransferPhase.Terminal:
      return "terminal";
    default:
      // 新增阶段默认按「等待对方接受」呈现——那是最无害的一档（不显示进度、不显示错误）。
      return "waiting_accept";
  }
}

/**
 * uniffi 的终态原因 → 共享包的中立字面量。
 *
 * 只有 `FatalError` 需要改名（共享包叫 `failed`）。**新增的终态原因默认落到 `failed`**——
 * 那是最保守的归类：宁可把一个新的失败原因显示成「出错」，也不要漏判成「已完成」。
 */
function toSharedTerminalReason(
  reason: MobileTerminalReason | undefined,
): SessionTerminalReason | null {
  switch (reason) {
    case MobileTerminalReason.Completed:
      return "completed";
    case MobileTerminalReason.Cancelled:
      return "cancelled";
    case MobileTerminalReason.Rejected:
      return "rejected";
    case MobileTerminalReason.Expired:
      return "expired";
    case undefined:
      return null;
    default:
      return "fatal_error";
  }
}

/**
 * uniffi 把逐文件状态给成裸 `string`（见 `MobileFileProgress`），共享包要的是真枚举。
 * **认不出来就返回 `undefined`**，让 L1 回落到按会话阶段推断——那比硬塞一个它不认识的
 * 值安全（后者会静默落到某个分支上）。
 */
function toSharedFileStatus(status: string): ProgressFileStatus | undefined {
  return status === "pending" ||
    status === "transferring" ||
    status === "completed"
    ? status
    : undefined;
}

export function fromSelectedFiles(
  files: readonly MobileTransferFile[],
): FileBrowserItem[] {
  return sharedFromSelectedFiles(
    files.map((file) => ({
      sourceId: file.sourceId,
      name: file.name,
      relativePath: file.relativePath,
      size: Number(file.size),
      // 发送来源在所有选择路径下都是可渲染的 file://（见 core/file-access + share-intent）。
      previewSource: file.sourceId,
    })),
  );
}

export function fromOfferFiles(
  sessionId: string,
  files: readonly MobileTransferOfferFile[],
): FileBrowserItem[] {
  return sharedFromOfferFiles(
    sessionId,
    files.map((file) => ({
      fileId: file.fileId,
      name: file.name,
      relativePath: file.relativePath ?? file.name,
      size: Number(file.size),
      isDirectory: file.isDirectory,
    })),
  );
}

export function fromProjection(
  projection: MobileTransferProjection,
  progress?: MobileTransferProgress | null,
): FileBrowserItem[] {
  return sharedFromProjectionFiles(
    projection.sessionId,
    projection.files.map((file) => ({
      fileId: file.fileId,
      name: file.name,
      relativePath: file.relativePath,
      size: Number(file.size),
      transferredBytes: Number(file.transferredBytes),
    })),
    {
      phase: toSharedPhase(projection.phase),
      terminalReason: toSharedTerminalReason(projection.terminalReason),
      progress: progress?.files.map((file) => ({
        fileId: file.fileId,
        transferred: Number(file.transferred),
        status: toSharedFileStatus(file.status),
      })),
    },
  );
}

export function fromInboxFiles(
  itemId: string,
  files: readonly MobileInboxFileEntry[],
): FileBrowserItem[] {
  return sharedFromInboxFiles(
    itemId,
    files.map((file) => ({
      id: file.id,
      transferFileId: file.transferFileId,
      name: file.name,
      relativePath: file.relativePath,
      size: Number(file.size),
      missing: file.missing,
      // 仅真为 file:// 时可缩略图；Android SAF content:// 不设（交系统打开）。
      // 「缺失就不给取图源」那一条由共享包统一兜住。
      ...(file.localPath.startsWith("file://")
        ? { previewSource: file.localPath }
        : {}),
    })),
  );
}
