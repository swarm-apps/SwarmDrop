/**
 * 传输操作 helper —— 暂停 / 取消 / 恢复
 * 供活动中心（/transfer）与发送流（/send、/send/share-target）共用。
 * 状态变化由后端 projection-update 事件回流（applyProjection），
 * 这里不做 loadProjections——避免冗余全量往返与乱序覆盖。
 */

import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import { getErrorMessage } from "@/lib/errors";
import { commands, type TransferProjection } from "@/lib/bindings";
import { useShareStore } from "@/stores/share-store";

/** 暂停传输 */
export async function doPauseTransfer(sessionId: string) {
  try {
    await commands.pauseTransfer(sessionId);
  } catch (err) {
    toast.error(getErrorMessage(err));
    throw err;
  }
}

/** 取消传输 */
export async function doCancelTransfer(
  sessionId: string,
  direction: "send" | "receive",
) {
  try {
    if (direction === "send") {
      await commands.cancelSend(sessionId);
    } else {
      await commands.cancelReceive(sessionId);
    }
    toast.success(t`已取消传输`);
  } catch (err) {
    toast.error(getErrorMessage(err));
    throw err;
  }
}

/** 恢复传输，返回新会话 ID */
export async function doResumeTransfer(sessionId: string): Promise<string> {
  const result = await commands.resumeTransfer(sessionId);
  if (result.direction !== "send" && result.direction !== "receive") {
    throw new Error(
      `resume_transfer returned invalid direction "${result.direction}" for ${sessionId}`,
    );
  }
  return result.sessionId;
}

/**
 * 重新发送：取回源文件绝对路径 → 塞进 share-store（携带原目标设备）供 share-target
 * 快捷发送流消费。与 [ExternalOpenHandler] 同一「发起快捷发送」入口；调用方在成功后
 * 自行 navigate 到 `/send/share-target`（与 doResumeTransfer 一样，导航留给组件）。
 * 找不到源路径时抛错，交调用方 toast。
 */
export async function doResendTransfer(
  projection: TransferProjection,
): Promise<void> {
  const paths = await commands.getTransferSourcePaths(projection.sessionId);
  if (paths.length === 0) {
    throw new Error(t`找不到原始文件路径，请重新选择文件发送`);
  }
  useShareStore.getState().setSources(
    paths.map((path) => ({ type: "path", path })),
    projection.peerId,
  );
}
