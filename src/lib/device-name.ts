import { commands } from "@/lib/bindings";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useNetworkStore } from "@/stores/network-store";

/**
 * 设备显示名 —— 优先用用户设置的 name，缺省时回退到系统 hostname。
 *
 * 适用于 Device / PairedDeviceInfo / OsInfo / lookupDeviceByCode 返回值等所有
 * 含 `{ name?, hostname }` 形状的对象。
 */
export function deviceDisplayName(d: {
  name?: string | null;
  hostname: string;
}): string {
  return d.name?.trim() || d.hostname;
}

/**
 * 设置设备名 —— 写后端 device_config.json + 同步前端缓存 + 节点在跑则重启
 * 让新名字通过 libp2p Identify `agent_version` 重新广播。
 *
 * 传空串/纯空白清空，回退到系统 hostname。
 */
export async function applyDeviceName(name: string): Promise<void> {
  const trimmed = name.trim();
  await commands.setDeviceName(trimmed || null);
  usePreferencesStore.setState({ deviceName: trimmed });

  const { status, stopNetwork, startNetwork } = useNetworkStore.getState();
  if (status === "running") {
    await stopNetwork();
    const ok = await startNetwork();
    if (!ok) {
      // startNetwork 失败时内部已 toast 原因；这里仅记录，避免误判节点已恢复。
      console.warn("[device-name] 改名后节点重启失败");
    }
  }
}

/**
 * 拉取后端持久化的设备名，覆盖前端缓存。应用启动 hydration 后调一次，确保
 * 跨设备/卸装重装时后端是 source of truth。
 */
export async function syncDeviceNameFromBackend(): Promise<void> {
  try {
    const backend = await commands.getDeviceName();
    if (backend !== null) {
      usePreferencesStore.setState({ deviceName: backend });
    }
  } catch (err) {
    console.warn("[device-name] sync from backend failed:", err);
  }
}
