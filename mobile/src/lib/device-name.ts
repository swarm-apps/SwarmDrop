import * as Device from "expo-device";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 设备显示名 —— 优先用用户起的 name，缺省时回退到系统 hostname。
 *
 * 适用于 MobileDevice / MobileRemoteDeviceInfo / 任何含 `{ name?, hostname }`
 * 形状的对象。
 */
export function deviceDisplayName(d: {
  name?: string | null;
  hostname: string;
}): string {
  return d.name?.trim() || d.hostname;
}

/**
 * 给 onboarding 输入框的默认值 —— 优先 expo-device 的 deviceName（Android 一般
 * 拿得到用户起的蓝牙/设备名；iOS 16+ 上多半是 "iPhone" 字符串），缺省时回退到
 * modelName（"iPhone 15 Pro" / "Pixel 8"）。最差用调用方传入的本地化兜底
 * （必填,由调用方负责本地化——lib 层保持 i18n-free）。
 */
export function suggestedDeviceName(fallbackName: string): string {
  return Device.deviceName?.trim() || Device.modelName?.trim() || fallbackName;
}

/**
 * 设置设备名 —— 写 preferences-store（AsyncStorage 持久化）+ 节点在跑则重启，
 * 让新名字通过 libp2p Identify `agent_version` 重新广播到对端。
 *
 * 传空串/纯空白清空，回退到系统 hostname。
 */
export async function applyDeviceName(name: string): Promise<void> {
  const trimmed = name.trim();
  usePreferencesStore.getState().setDeviceName(trimmed);

  const { runtimeState, shutdownNode, startNode } =
    useMobileCoreStore.getState();
  if (runtimeState === "running") {
    const shutdown = await shutdownNode();
    if (!shutdown.ok) {
      throw new Error(shutdown.error);
    }
    const start = await startNode();
    if (!start.ok) {
      throw new Error(start.error);
    }
  }
}
