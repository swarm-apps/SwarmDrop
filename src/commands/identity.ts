/**
 * Identity commands
 *
 * 注：IdentityState 不直接 re-export 自 @/lib/bindings —— bindings 的
 * PairedDeviceInfo 比 secret-store 持久化的 PairedDevice 多 `pairedAt`
 * 字段，前端为兼容旧持久化数据保留较松的形状。其他无漂移类型直接
 * re-export 自 bindings。
 */

import { invoke } from "@tauri-apps/api/core";
import type { PairedDevice } from "@/stores/secret-store";

export interface IdentityState {
  keypair: number[];
  deviceId: string;
  pairedDevices: PairedDevice[];
  created: boolean;
}

export async function initializeIdentity(): Promise<IdentityState> {
  return await invoke("initialize_identity");
}

export async function generateKeypair(): Promise<number[]> {
  return await invoke("generate_keypair");
}

export async function registerKeypair(keypair: number[]): Promise<string> {
  return await invoke("register_keypair", { keypair });
}
