/**
 * Pairing commands
 * 设备配对相关命令
 *
 * 类型从 specta 生成的 @/lib/bindings re-export，避免前后端漂移。
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  DeviceInfo,
  PairingCodeInfo,
  PairingMethod,
  PairingRefuseReason,
  PairingResponse,
  ShareCodeRecord,
} from "@/lib/bindings";
import type { PeerId } from "./network";

export type {
  DeviceInfo,
  PairingCodeInfo,
  PairingMethod,
  PairingRefuseReason,
  PairingResponse,
  ShareCodeRecord,
};

/**
 * 生成配对码，发布到 DHT 供对端查询
 *
 * @param expiresInSecs - 配对码有效期（秒），默认 300
 */
export async function generatePairingCode(
  expiresInSecs?: number,
): Promise<PairingCodeInfo> {
  return invoke<PairingCodeInfo>("generate_pairing_code", { expiresInSecs });
}

/**
 * 通过配对码查询对端设备信息
 *
 * @param code - 6 位配对码
 */
export async function getDeviceInfo(code: string): Promise<DeviceInfo> {
  return invoke<DeviceInfo>("get_device_info", { code });
}

/**
 * 向对端发起配对请求
 *
 * @param peerId - 对端 Peer ID
 * @param method - 配对方式
 * @param addrs - 对端可达地址（可选），用于跨网络场景下注册地址后直接 dial
 */
export async function requestPairing(
  peerId: PeerId,
  method: PairingMethod,
  addrs?: string[],
): Promise<PairingResponse> {
  return invoke<PairingResponse>("request_pairing", { peerId, method, addrs });
}

/**
 * 取消与指定设备的配对（同步更新后端运行时状态）
 *
 * 节点未运行时静默成功，前端应同时更新 Stronghold 持久化。
 */
export async function removePairedDevice(peerId: PeerId): Promise<void> {
  return invoke("remove_paired_device", { peerId });
}

/**
 * 响应收到的配对请求（接受/拒绝）
 *
 * @param pendingId - 请求标识（来自 InboundRequest 事件）
 * @param method - 配对方式
 * @param response - 接受或拒绝
 */
export async function respondPairingRequest(
  pendingId: number,
  method: PairingMethod,
  response: PairingResponse,
): Promise<void> {
  return invoke("respond_pairing_request", { pendingId, method, response });
}
