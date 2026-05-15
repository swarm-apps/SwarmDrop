/**
 * Network commands
 * P2P 网络相关命令
 *
 * 类型从 specta 生成的 @/lib/bindings re-export，避免前后端漂移。
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionType,
  Device,
  DeviceListResult,
  DeviceStatus,
  NetworkStatus,
  NodeStatus,
} from "@/lib/bindings";
import type { PairedDevice } from "@/stores/secret-store";

export type {
  ConnectionType,
  Device,
  DeviceListResult,
  DeviceStatus,
  NetworkStatus,
  NodeStatus,
};

/** Peer ID (libp2p 节点标识) */
export type PeerId = string;
/** Multiaddr (libp2p 多地址) */
export type Multiaddr = string;
/** NAT 状态。bindings 把它映射成 string，前端这里收紧到字面量联合类型。 */
export type NatStatus = "public" | "unknown";

/**
 * 启动 P2P 网络节点
 * 注意：调用前必须确保 keypair 已通过 register_keypair 注册到后端
 *
 * @param pairedDevices - 已配对设备列表（从 Stronghold 读取）
 */
export async function start(
  pairedDevices: PairedDevice[],
  customBootstrapNodes?: string[],
): Promise<void> {
  await invoke("start", { pairedDevices, customBootstrapNodes });
}

/** 关闭 P2P 网络节点 */
export async function shutdown(): Promise<void> {
  await invoke("shutdown");
}

/**
 * 获取设备列表
 * @param filter - 过滤器: "all" | "connected" | "paired"，默认 "connected"
 */
export async function listDevices(
  filter?: "all" | "connected" | "paired",
): Promise<DeviceListResult> {
  return invoke("list_devices", { filter });
}

/** 获取网络状态 */
export async function getNetworkStatus(): Promise<NetworkStatus> {
  return invoke("get_network_status");
}
