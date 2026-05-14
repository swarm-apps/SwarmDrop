/**
 * Identity commands
 */

import { invoke } from "@tauri-apps/api/core";
import type { PairedDevice } from "@/stores/secret-store";

export interface IdentityState {
  keypair: number[];
  deviceId: string;
  pairedDevices: PairedDevice[];
  created: boolean;
}

/**
 * 从系统 keychain 初始化设备身份；不存在时由后端自动生成并保存。
 */
export async function initializeIdentity(): Promise<IdentityState> {
  return await invoke("initialize_identity");
}

/**
 * 生成新的 Ed25519 密钥对。
 */
export async function generateKeypair(): Promise<number[]> {
  return await invoke("generate_keypair");
}

/**
 * 注册密钥对到后端状态管理。
 */
export async function registerKeypair(keypair: number[]): Promise<string> {
  return await invoke("register_keypair", { keypair });
}
